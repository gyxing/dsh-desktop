use super::download::DownloadError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ParsedContentRange {
    pub start: u64,
    pub end: u64,
    pub total: u64,
}

/// 只接受完整的`bytes start-end/total`格式，并验证范围数值关系。
pub(super) fn parse_content_range(value: &str) -> Result<ParsedContentRange, DownloadError> {
    let value = value
        .strip_prefix("bytes ")
        .ok_or_else(|| DownloadError::InvalidResponse(format!("Content-Range格式无效：{value}")))?;
    let (range, total) = value.split_once('/').ok_or_else(|| {
        DownloadError::InvalidResponse(format!("Content-Range缺少总大小：{value}"))
    })?;
    let (start, end) = range
        .split_once('-')
        .ok_or_else(|| DownloadError::InvalidResponse(format!("Content-Range缺少范围：{value}")))?;
    let start = start
        .parse::<u64>()
        .map_err(|_| DownloadError::InvalidResponse(format!("Content-Range起点无效：{value}")))?;
    let end = end
        .parse::<u64>()
        .map_err(|_| DownloadError::InvalidResponse(format!("Content-Range终点无效：{value}")))?;
    let total = total
        .parse::<u64>()
        .map_err(|_| DownloadError::InvalidResponse(format!("Content-Range总大小无效：{value}")))?;
    if end < start || total == 0 || end >= total {
        return Err(DownloadError::InvalidResponse(format!(
            "Content-Range数值无效：{value}"
        )));
    }
    Ok(ParsedContentRange { start, end, total })
}
