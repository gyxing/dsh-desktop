/// 更新来源决定失败时是否主动打扰用户。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateCheckSource {
    Automatic,
    Manual,
}

/// 更新状态只服务桌面壳和托盘，不向远程 DSH 页面暴露。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpdateStatus {
    Idle,
    Checking {
        source: UpdateCheckSource,
    },
    UpToDate,
    Available {
        version: String,
        notes: Option<String>,
    },
    Downloading {
        version: String,
        downloaded: u64,
        total: Option<u64>,
        bytes_per_second: Option<u64>,
        eta_seconds: Option<u64>,
    },
    Retrying {
        version: String,
        downloaded: u64,
        total: Option<u64>,
        bytes_per_second: Option<u64>,
        eta_seconds: Option<u64>,
        next_attempt: usize,
        max_attempts: usize,
    },
    Verifying {
        version: String,
    },
    Installing {
        version: String,
    },
    Failed {
        source: UpdateCheckSource,
        message: String,
    },
}
