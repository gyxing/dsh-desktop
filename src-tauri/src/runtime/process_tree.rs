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

    /// 持有 Windows Job Object；关闭句柄时系统会终止完整子进程树。
    pub struct ProcessTreeGuard {
        job: isize,
    }

    impl ProcessTreeGuard {
        /// 创建 Job Object 并立即把 Sidecar 根进程纳入管理。
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

#[cfg(windows)]
pub use platform::ProcessTreeGuard;

#[cfg(not(windows))]
pub struct ProcessTreeGuard;

#[cfg(not(windows))]
impl ProcessTreeGuard {
    /// 其他平台将在对应移植阶段替换为原生进程组管理。
    pub fn attach(_process_id: u32) -> std::io::Result<Self> {
        Ok(Self)
    }
}
