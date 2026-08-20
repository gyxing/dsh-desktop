use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{
    header::{
        HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, RANGE,
    },
    Client, StatusCode,
};
use tauri_plugin_updater::Update;
use thiserror::Error;
use url::Url;

use super::range::parse_content_range;

const MAX_DOWNLOAD_ATTEMPTS: usize = 5;
const MAX_UPDATE_BYTES: u64 = 512 * 1024 * 1024;
const UPDATER_USER_AGENT: &str = "DSH Desktop Updater";

/// 断点续传只接受可恢复网络错误；响应偏移和签名异常必须立即失败。
#[derive(Debug, Error)]
pub enum DownloadError {
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error("下载请求返回状态 {0}")]
    Status(StatusCode),
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
    /// 仅网络传输、可重试状态码和正文不完整允许继续请求。
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
            Self::Incomplete { .. } => true,
            Self::InvalidResponse(_) | Self::Configuration(_) | Self::Signature(_) => false,
        }
    }
}

/// 保留已接收字节，并在可恢复错误后通过Range从精确偏移继续下载。
pub async fn download_with_resume<P, R>(
    update: &Update,
    on_progress: P,
    on_retry: R,
) -> Result<Vec<u8>, DownloadError>
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

    download_url_with_resume(
        &client,
        update.download_url.clone(),
        headers,
        on_progress,
        on_retry,
    )
    .await
}

/// 下载指定URL并执行严格Range拼接，供Tauri包装和可控断流验收共用。
pub(crate) async fn download_url_with_resume<P, R>(
    client: &Client,
    download_url: Url,
    headers: HeaderMap,
    mut on_progress: P,
    mut on_retry: R,
) -> Result<Vec<u8>, DownloadError>
where
    P: FnMut(u64, Option<u64>),
    R: FnMut(u64, Option<u64>, usize, usize, &DownloadError),
{
    let mut buffer = Vec::new();
    let mut expected_total = None;
    for attempt in 1..=MAX_DOWNLOAD_ATTEMPTS {
        match download_attempt(
            &client,
            &download_url,
            &headers,
            &mut buffer,
            &mut expected_total,
            &mut on_progress,
        )
        .await
        {
            Ok(()) => return Ok(buffer),
            Err(error) if attempt < MAX_DOWNLOAD_ATTEMPTS && error.is_retryable() => {
                on_retry(
                    buffer.len() as u64,
                    expected_total,
                    attempt + 1,
                    MAX_DOWNLOAD_ATTEMPTS,
                    &error,
                );
                tokio::time::sleep(retry_delay(attempt)).await;
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("断点续传循环必须返回结果")
}

async fn download_attempt<P>(
    client: &Client,
    download_url: &Url,
    headers: &HeaderMap,
    buffer: &mut Vec<u8>,
    expected_total: &mut Option<u64>,
    on_progress: &mut P,
) -> Result<(), DownloadError>
where
    P: FnMut(u64, Option<u64>),
{
    let requested_offset = buffer.len() as u64;
    if expected_total.is_some_and(|total| requested_offset > total) {
        return Err(DownloadError::InvalidResponse(format!(
            "续传偏移超过总大小：偏移 {requested_offset}，总大小 {}",
            expected_total.unwrap_or_default()
        )));
    }
    if expected_total.is_some_and(|total| requested_offset == total) {
        return Ok(());
    }

    let mut request = client.get(download_url.clone()).headers(headers.clone());
    if requested_offset > 0 {
        request = request.header(RANGE, format!("bytes={requested_offset}-"));
    }
    let response = request.send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(DownloadError::Status(status));
    }

    match status {
        StatusCode::PARTIAL_CONTENT => {
            let content_range = response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| {
                    DownloadError::InvalidResponse("206响应缺少Content-Range".to_string())
                })?;
            let parsed = parse_content_range(content_range)?;
            if parsed.start != requested_offset {
                return Err(DownloadError::InvalidResponse(format!(
                    "期望从 {requested_offset} 续传，实际从 {} 返回",
                    parsed.start
                )));
            }
            if let Some(total) = expected_total {
                if *total != parsed.total {
                    return Err(DownloadError::InvalidResponse(format!(
                        "续传总大小发生变化：原值 {total}，新值 {}",
                        parsed.total
                    )));
                }
            }
            if let Some(content_length) = response_content_length(&response) {
                let range_length = parsed.end - parsed.start + 1;
                if content_length != range_length {
                    return Err(DownloadError::InvalidResponse(format!(
                        "续传Content-Length与Content-Range不一致：{content_length} != {range_length}"
                    )));
                }
            }
            ensure_size_allowed(parsed.total)?;
            *expected_total = Some(parsed.total);
        }
        StatusCode::OK => {
            // 服务端忽略Range时只能清空旧缓冲并把本次200响应作为完整文件重新接收。
            if requested_offset > 0 {
                buffer.clear();
            }
            *expected_total = response_content_length(&response);
            if let Some(total) = expected_total {
                ensure_size_allowed(*total)?;
            }
        }
        _ => {
            return Err(DownloadError::InvalidResponse(format!(
                "下载响应状态不受支持：{status}"
            )));
        }
    }

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let next_size = buffer.len() as u64 + chunk.len() as u64;
        ensure_size_allowed(next_size)?;
        buffer.extend_from_slice(&chunk);
        on_progress(buffer.len() as u64, *expected_total);
    }

    if let Some(total) = *expected_total {
        let downloaded = buffer.len() as u64;
        if downloaded < total {
            return Err(DownloadError::Incomplete { downloaded, total });
        }
        if downloaded > total {
            return Err(DownloadError::InvalidResponse(format!(
                "下载字节超过声明总大小：已下载 {downloaded}，总大小 {total}"
            )));
        }
    }
    Ok(())
}

fn build_client(update: &Update) -> Result<Client, DownloadError> {
    install_crypto_provider();
    let mut builder = Client::builder().user_agent(UPDATER_USER_AGENT);
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

fn response_content_length(response: &reqwest::Response) -> Option<u64> {
    response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}

fn ensure_size_allowed(size: u64) -> Result<(), DownloadError> {
    if size > MAX_UPDATE_BYTES {
        return Err(DownloadError::InvalidResponse(format!(
            "更新包超过512 MB安全上限：{size}字节"
        )));
    }
    Ok(())
}

fn retry_delay(failed_attempt: usize) -> Duration {
    Duration::from_secs(1_u64 << (failed_attempt - 1).min(3))
}
