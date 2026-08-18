use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use super::status::{UpdateCheckSource, UpdateStatus};

/// 保存桌面更新状态，并保证同一时间只有一个检查或安装任务。
pub struct UpdateManager {
    status: Mutex<UpdateStatus>,
    in_progress: AtomicBool,
}

impl UpdateManager {
    /// 创建尚未检查更新的初始状态。
    pub fn new() -> Self {
        Self {
            status: Mutex::new(UpdateStatus::Idle),
            in_progress: AtomicBool::new(false),
        }
    }

    /// 返回当前更新状态的快照。
    pub fn status(&self) -> UpdateStatus {
        self.status
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    /// 原子替换当前更新状态。
    pub fn set_status(&self, status: UpdateStatus) {
        *self
            .status
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = status;
    }

    /// 尝试开始更新任务，返回的门闩在所有退出路径都会释放。
    pub fn try_begin(self: &Arc<Self>, source: UpdateCheckSource) -> Option<UpdateRunGuard> {
        self.in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        self.set_status(UpdateStatus::Checking { source });
        Some(UpdateRunGuard {
            manager: Arc::clone(self),
        })
    }
}

impl Default for UpdateManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 通过 Drop 保证错误分支不会永久锁死后续更新检查。
pub struct UpdateRunGuard {
    manager: Arc<UpdateManager>,
}

impl Drop for UpdateRunGuard {
    fn drop(&mut self) {
        self.manager.in_progress.store(false, Ordering::Release);
    }
}
