use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseAction {
    Hide,
    Exit,
}

/// 区分用户关闭窗口与托盘明确退出，避免系统退出被“隐藏窗口”逻辑拦截。
pub struct AppLifecycle {
    quitting: AtomicBool,
}

impl AppLifecycle {
    pub fn new() -> Self {
        Self {
            quitting: AtomicBool::new(false),
        }
    }

    pub fn close_action(&self) -> CloseAction {
        if self.quitting.load(Ordering::Acquire) {
            CloseAction::Exit
        } else {
            CloseAction::Hide
        }
    }

    /// 返回是否由本次调用首次发起退出。
    pub fn request_quit(&self) -> bool {
        !self.quitting.swap(true, Ordering::AcqRel)
    }
}

#[cfg(test)]
mod tests {
    use super::{AppLifecycle, CloseAction};

    #[test]
    fn ordinary_window_close_hides_the_application() {
        let lifecycle = AppLifecycle::new();

        assert_eq!(lifecycle.close_action(), CloseAction::Hide);
    }

    #[test]
    fn explicit_quit_allows_the_window_and_process_to_exit() {
        let lifecycle = AppLifecycle::new();

        assert!(lifecycle.request_quit());
        assert_eq!(lifecycle.close_action(), CloseAction::Exit);
        assert!(!lifecycle.request_quit(), "重复退出请求应保持幂等");
    }
}
