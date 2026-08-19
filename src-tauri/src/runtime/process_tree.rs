#[cfg(windows)]
mod platform {
    use std::{io, mem::size_of, ptr};

    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
            Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE},
        },
    };

    /// 持有Windows Job Object；关闭句柄时系统会终止完整子进程树。
    pub struct ProcessTreeGuard {
        job: isize,
    }

    impl ProcessTreeGuard {
        /// 创建Job Object并立即把Sidecar根进程纳入管理。
        pub fn attach(process_id: u32) -> io::Result<Self> {
            unsafe {
                let job = CreateJobObjectW(ptr::null(), ptr::null());
                if job.is_null() {
                    return Err(io::Error::last_os_error());
                }

                let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let configured = SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    ptr::from_ref(&limits).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if configured == 0 {
                    return close_with_error(job);
                }

                let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, process_id);
                if process.is_null() {
                    return close_with_error(job);
                }

                let assigned = AssignProcessToJobObject(job, process);
                let assign_error = (assigned == 0).then(io::Error::last_os_error);
                CloseHandle(process);
                if let Some(error) = assign_error {
                    CloseHandle(job);
                    return Err(error);
                }

                Ok(Self { job: job as isize })
            }
        }

        /// 消费守卫并关闭Job Object，让系统同步回收完整进程树。
        pub fn terminate(self) {}
    }

    impl Drop for ProcessTreeGuard {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.job as HANDLE);
            }
        }
    }

    unsafe fn close_with_error(job: HANDLE) -> io::Result<ProcessTreeGuard> {
        let error = io::Error::last_os_error();
        CloseHandle(job);
        Err(error)
    }
}

#[cfg(unix)]
mod platform {
    use std::{io, thread, time::Duration};

    /// 持有Unix进程组ID；显式退出先TERM，超时后再KILL。
    pub struct ProcessTreeGuard {
        process_group: i32,
        armed: bool,
    }

    impl ProcessTreeGuard {
        /// Sidecar在spawn前已设置自己的进程组，PID即进程组ID。
        pub fn attach(process_id: u32) -> io::Result<Self> {
            let process_group = i32::try_from(process_id)
                .map_err(|_| io::Error::other("Sidecar进程ID超出Unix进程组范围"))?;
            Ok(Self {
                process_group,
                armed: true,
            })
        }

        /// 请求进程组优雅退出，并在两秒后做有界强制回收。
        pub fn terminate(mut self) {
            let process_group = self.process_group;
            self.armed = false;
            unsafe {
                libc::kill(-process_group, libc::SIGTERM);
            }
            thread::spawn(move || {
                thread::sleep(Duration::from_secs(2));
                unsafe {
                    libc::kill(-process_group, libc::SIGKILL);
                }
            });
        }
    }

    impl Drop for ProcessTreeGuard {
        fn drop(&mut self) {
            if self.armed {
                unsafe {
                    libc::kill(-self.process_group, libc::SIGKILL);
                }
            }
        }
    }
}

pub use platform::ProcessTreeGuard;
