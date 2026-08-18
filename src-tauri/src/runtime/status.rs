use serde::Serialize;

/// 前端可依赖的稳定失败分类，具体系统错误只进入脱敏诊断。
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeErrorCode {
    RuntimeMissing,
    SpawnFailed,
    ProcessTreeFailed,
    ReadinessInvalid,
    StartupTimeout,
    HttpUnreachable,
    PageLoadFailed,
    ProcessExited,
    RuntimeCommunication,
}

/// 描述内置 DSH 运行时当前所处的生命周期阶段。
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "phase", rename_all = "lowercase")]
pub enum RuntimeStatus {
    Starting {
        message: String,
    },
    Probing {
        message: String,
    },
    Loading {
        message: String,
    },
    Ready {
        message: String,
        url: String,
    },
    Failed {
        code: RuntimeErrorCode,
        message: String,
    },
    Exited {
        code: RuntimeErrorCode,
        message: String,
    },
}

impl RuntimeStatus {
    /// 创建默认启动状态。
    pub fn starting() -> Self {
        Self::Starting {
            message: "正在启动 DeepSeek Harness…".to_string(),
        }
    }

    /// 创建正在检查本机 HTTP 服务的状态。
    pub fn probing() -> Self {
        Self::Probing {
            message: "正在检查 DeepSeek Harness 服务…".to_string(),
        }
    }

    /// 创建正在加载 DSH Web 页面的状态。
    pub fn loading() -> Self {
        Self::Loading {
            message: "正在加载 DeepSeek Harness 页面…".to_string(),
        }
    }

    pub fn failed(code: RuntimeErrorCode, message: impl Into<String>) -> Self {
        Self::Failed {
            code,
            message: message.into(),
        }
    }

    /// 判断当前状态是否仍在等待就绪信号。
    pub fn is_starting(&self) -> bool {
        matches!(
            self,
            Self::Starting { .. } | Self::Probing { .. } | Self::Loading { .. }
        )
    }

    /// 判断当前状态是否已经进入远程 Web 界面。
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{RuntimeErrorCode, RuntimeStatus};

    #[test]
    fn pending_phases_remain_startup_states_until_the_page_is_ready() {
        assert!(RuntimeStatus::starting().is_starting());
        assert!(RuntimeStatus::probing().is_starting());
        assert!(RuntimeStatus::loading().is_starting());
    }

    #[test]
    fn failed_status_serializes_a_stable_error_code_for_the_frontend() {
        let status = RuntimeStatus::failed(RuntimeErrorCode::StartupTimeout, "启动超时");

        assert_eq!(
            serde_json::to_value(status).expect("状态应可序列化"),
            json!({
                "phase": "failed",
                "code": "STARTUP_TIMEOUT",
                "message": "启动超时"
            })
        );
    }
}
