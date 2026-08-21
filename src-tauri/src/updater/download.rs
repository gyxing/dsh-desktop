use std::time::Duration;

use reqwest::{
    header::{HeaderValue, ACCEPT, ACCEPT_ENCODING, RANGE},
    Client, StatusCode,
};
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::Update;
use thiserror::Error;

use super::{
    cache::{CacheError, DownloadCache, DownloadIdentity},
    transfer::download_url_to_cache_with_policy,
};

const MAX_DOWNLOAD_ATTEMPTS: usize = 5;
pub(super) const MAX_UPDATE_BYTES: u64 = 512 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const NO_PROGRESS_TIMEOUT: Duration = Duration::from_secs(45);
const UPDATER_USER_AGENT: &str = "DSH Desktop Updater";

#[derive(Clone, Copy)]
pub(super) struct DownloadPolicy {
    pub max_attempts: usize,
    pub response_timeout: Duration,
    pub no_progress_timeout: Duration,
    pub retry_delay_enabled: bool,
}

impl DownloadPolicy {
    fn production() -> Self {
        Self {
            max_attempts: MAX_DOWNLOAD_ATTEMPTS,
            response_timeout: RESPONSE_TIMEOUT,
            no_progress_timeout: NO_PROGRESS_TIMEOUT,
            retry_delay_enabled: true,
        }
    }

    #[cfg(test)]
    pub fn for_test(max_attempts: usize, no_progress_timeout: Duration) -> Self {
        Self {
            max_attempts,
            response_timeout: Duration::from_secs(1),
            no_progress_timeout,
            retry_delay_enabled: false,
        }
    }
}

/// 下载结果保留在应用私有目录，验签和安装完成后再清理。
pub struct DownloadedUpdate {
    cache: DownloadCache,
}

impl DownloadedUpdate {
    pub fn path(&self) -> &std::path::Path {
        self.cache.part_path()
    }

    pub fn size(&self) -> u64 {
        self.cache.downloaded_len()
    }

    pub async fn read_for_install(&self) -> Result<Vec<u8>, DownloadError> {
        let bytes = tokio::fs::read(self.path()).await?;
        if bytes.len() as u64 != self.size() {
            return Err(DownloadError::InvalidResponse(format!(
                "安装前缓存大小发生变化：期望 {}，实际 {}",
                self.size(),
                bytes.len()
            )));
        }
        Ok(bytes)
    }

    pub fn clear(self) -> Result<(), DownloadError> {
        self.cache.clear()?;
        Ok(())
    }
}

/// 断点续传只接受可恢复网络错误；响应偏移、缓存和签名异常必须立即失败。
#[derive(Debug, Error)]
pub enum DownloadError {
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Cache(#[from] CacheError),
    #[error("下载请求返回状态 {0}")]
    Status(StatusCode),
    #[error("下载请求在 {seconds} 秒内没有收到响应")]
    ResponseTimeout { seconds: u64 },
    #[error("下载连接在 {seconds} 秒内没有新数据")]
    NoProgressTimeout { seconds: u64 },
    #[error("下载响应不符合断点续传约束：{0}")]
    InvalidResponse(String),
    #[error("下载结果不完整：已下载 {downloaded} 字节，总大小 {total} 字节")]
    Incomplete { downloaded: u64, total: u64 },
    #[error("更新器配置无效：{0}")]
    Configuration(String),
    #[error("更新签名校验失败：{0}")]
    Signature(String),
}

impl DownloadError {
    /// 仅网络传输、超时、可重试状态码和正文不完整允许继续请求。
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Request(error) => {
                error.is_body() || error.is_connect() || error.is_decode() || error.is_timeout()
            }
            Self::Status(status) => {
                status.is_server_error()
                    || *status == StatusCode::REQUEST_TIMEOUT
                    || *status == StatusCode::TOO_MANY_REQUESTS
            }
            Self::ResponseTimeout { .. }
            | Self::NoProgressTimeout { .. }
            | Self::Incomplete { .. } => true,
            Self::Io(_)
            | Self::Cache(_)
            | Self::InvalidResponse(_)
            | Self::Configuration(_)
            | Self::Signature(_) => false,
        }
    }
}

/// 把更新包保存到应用私有缓存，并在重启或网络中断后从精确偏移继续。
pub async fn download_with_resume<P, R>(
    app: &AppHandle,
    update: &Update,
    on_progress: P,
    on_retry: R,
) -> Result<DownloadedUpdate, DownloadError>
where
    P: FnMut(u64, Option<u64>),
    R: FnMut(u64, Option<u64>, usize, usize, &DownloadError),
{
    let client = build_client(update)?;
    let mut headers = update.headers.clone();
    if !headers.contains_key(ACCEPT) {
        headers.insert(ACCEPT, HeaderValue::from_static("application/octet-stream"));
    }
    headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
    headers.remove(RANGE);

    let cache_directory = app
        .path()
        .app_local_data_dir()
        .map_err(|error| DownloadError::Configuration(error.to_string()))?
        .join("updater");
    let identity = DownloadIdentity {
        version: update.version.clone(),
        url: update.download_url.to_string(),
        signature: update.signature.clone(),
    };
    let mut cache = DownloadCache::prepare(&cache_directory, identity, MAX_UPDATE_BYTES)?;
    download_url_to_cache_with_policy(
        &client,
        update.download_url.clone(),
        headers,
        &mut cache,
        DownloadPolicy::production(),
        on_progress,
        on_retry,
    )
    .await?;
    Ok(DownloadedUpdate { cache })
}

fn build_client(update: &Update) -> Result<Client, DownloadError> {
    install_crypto_provider();
    let mut builder = Client::builder()
        .user_agent(UPDATER_USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT);
    if let Some(timeout) = update.timeout {
        builder = builder.timeout(timeout);
    }
    if update.no_proxy {
        builder = builder.no_proxy();
    } else if let Some(proxy) = &update.proxy {
        builder = builder.proxy(reqwest::Proxy::all(proxy.as_str())?);
    }
    Ok(builder.build()?)
}

/// 与Tauri Updater使用同一个ring provider，并允许测试或独立调用先于更新检查执行。
pub(crate) fn install_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;
