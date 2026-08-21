use std::{io::SeekFrom, time::Duration};

use futures_util::StreamExt;
use reqwest::{
    header::{HeaderMap, CONTENT_LENGTH, CONTENT_RANGE, RANGE},
    Client, StatusCode,
};
use tokio::{
    fs::OpenOptions,
    io::{AsyncSeekExt, AsyncWriteExt},
};
use url::Url;

use super::{
    cache::DownloadCache,
    download::{DownloadError, DownloadPolicy, MAX_UPDATE_BYTES},
    range::parse_content_range,
};

pub(super) async fn download_url_to_cache_with_policy<P, R>(
    client: &Client,
    download_url: Url,
    headers: HeaderMap,
    cache: &mut DownloadCache,
    policy: DownloadPolicy,
    mut on_progress: P,
    mut on_retry: R,
) -> Result<(), DownloadError>
where
    P: FnMut(u64, Option<u64>),
    R: FnMut(u64, Option<u64>, usize, usize, &DownloadError),
{
    on_progress(cache.downloaded_len(), cache.expected_total());
    for attempt in 1..=policy.max_attempts {
        match download_attempt(
            client,
            &download_url,
            &headers,
            cache,
            policy,
            &mut on_progress,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(error) if attempt < policy.max_attempts && error.is_retryable() => {
                on_retry(
                    cache.downloaded_len(),
                    cache.expected_total(),
                    attempt + 1,
                    policy.max_attempts,
                    &error,
                );
                if policy.retry_delay_enabled {
                    tokio::time::sleep(retry_delay(attempt)).await;
                }
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
    cache: &mut DownloadCache,
    policy: DownloadPolicy,
    on_progress: &mut P,
) -> Result<(), DownloadError>
where
    P: FnMut(u64, Option<u64>),
{
    let requested_offset = cache.downloaded_len();
    validate_cached_offset(requested_offset, cache.expected_total())?;
    if cache
        .expected_total()
        .is_some_and(|total| requested_offset == total)
    {
        return Ok(());
    }

    let mut request = client.get(download_url.clone()).headers(headers.clone());
    if requested_offset > 0 {
        request = request.header(RANGE, format!("bytes={requested_offset}-"));
    }
    let response = tokio::time::timeout(policy.response_timeout, request.send())
        .await
        .map_err(|_| DownloadError::ResponseTimeout {
            seconds: policy.response_timeout.as_secs(),
        })??;
    let status = response.status();
    if !status.is_success() {
        return Err(DownloadError::Status(status));
    }

    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(cache.part_path())
        .await?;
    apply_response_metadata(status, &response, requested_offset, cache, &mut file).await?;
    file.seek(SeekFrom::Start(cache.downloaded_len())).await?;
    stream_response(response, cache, policy, &mut file, on_progress).await?;
    validate_final_size(cache.downloaded_len(), cache.expected_total())
}

async fn stream_response<P>(
    response: reqwest::Response,
    cache: &mut DownloadCache,
    policy: DownloadPolicy,
    file: &mut tokio::fs::File,
    on_progress: &mut P,
) -> Result<(), DownloadError>
where
    P: FnMut(u64, Option<u64>),
{
    let mut stream = response.bytes_stream();
    loop {
        let next = tokio::time::timeout(policy.no_progress_timeout, stream.next())
            .await
            .map_err(|_| DownloadError::NoProgressTimeout {
                seconds: policy.no_progress_timeout.as_secs(),
            })?;
        let Some(chunk) = next else {
            break;
        };
        let chunk = chunk?;
        let next_size = cache.downloaded_len() + chunk.len() as u64;
        ensure_size_allowed(next_size)?;
        file.write_all(&chunk).await?;
        cache.set_downloaded_len(next_size);
        on_progress(next_size, cache.expected_total());
    }
    file.flush().await?;
    Ok(())
}

async fn apply_response_metadata(
    status: StatusCode,
    response: &reqwest::Response,
    requested_offset: u64,
    cache: &mut DownloadCache,
    file: &mut tokio::fs::File,
) -> Result<(), DownloadError> {
    match status {
        StatusCode::PARTIAL_CONTENT => apply_partial_metadata(response, requested_offset, cache)?,
        StatusCode::OK => {
            // 服务端忽略Range时清空旧文件，把本次200响应作为完整文件重新接收。
            if requested_offset > 0 {
                file.set_len(0).await?;
                file.seek(SeekFrom::Start(0)).await?;
                cache.set_downloaded_len(0);
            }
            let total = response_content_length(response);
            if let Some(total) = total {
                ensure_size_allowed(total)?;
            }
            cache.set_expected_total(total)?;
        }
        _ => {
            return Err(DownloadError::InvalidResponse(format!(
                "下载响应状态不受支持：{status}"
            )));
        }
    }
    Ok(())
}

fn apply_partial_metadata(
    response: &reqwest::Response,
    requested_offset: u64,
    cache: &mut DownloadCache,
) -> Result<(), DownloadError> {
    let content_range = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| DownloadError::InvalidResponse("206响应缺少Content-Range".to_string()))?;
    let parsed = parse_content_range(content_range)?;
    if parsed.start != requested_offset {
        return Err(DownloadError::InvalidResponse(format!(
            "期望从 {requested_offset} 续传，实际从 {} 返回",
            parsed.start
        )));
    }
    if cache
        .expected_total()
        .is_some_and(|total| total != parsed.total)
    {
        return Err(DownloadError::InvalidResponse(format!(
            "续传总大小发生变化：原值 {}，新值 {}",
            cache.expected_total().unwrap_or_default(),
            parsed.total
        )));
    }
    if let Some(content_length) = response_content_length(response) {
        let range_length = parsed.end - parsed.start + 1;
        if content_length != range_length {
            return Err(DownloadError::InvalidResponse(format!(
                "续传Content-Length与Content-Range不一致：{content_length} != {range_length}"
            )));
        }
    }
    ensure_size_allowed(parsed.total)?;
    cache.set_expected_total(Some(parsed.total))?;
    Ok(())
}

fn validate_cached_offset(offset: u64, total: Option<u64>) -> Result<(), DownloadError> {
    if total.is_some_and(|total| offset > total) {
        return Err(DownloadError::InvalidResponse(format!(
            "续传偏移超过总大小：偏移 {offset}，总大小 {}",
            total.unwrap_or_default()
        )));
    }
    Ok(())
}

fn validate_final_size(downloaded: u64, total: Option<u64>) -> Result<(), DownloadError> {
    if let Some(total) = total {
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
