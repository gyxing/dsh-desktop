use thiserror::Error;
use url::Url;

const READY_PREFIX: &str = "dsh web: ";

#[derive(Debug, Error)]
pub enum ReadinessError {
    #[error("DSH 返回的就绪地址格式无效")]
    InvalidUrl,
    #[error("DSH 返回了非本机就绪地址")]
    NonLocalUrl,
}

/// 从 DSH 标准输出中提取经过严格限制的本机 Web 地址。
pub fn parse_readiness(line: &str) -> Result<Option<Url>, ReadinessError> {
    let line = line.trim();
    let Some(value) = line.strip_prefix(READY_PREFIX) else {
        return Ok(None);
    };

    let url = Url::parse(value).map_err(|_| ReadinessError::InvalidUrl)?;
    let valid_port = url.port().is_some_and(|port| port > 0);
    let is_local = url.scheme() == "http"
        && url.host_str() == Some("127.0.0.1")
        && valid_port
        && url.username().is_empty()
        && url.password().is_none();

    if !is_local {
        return Err(ReadinessError::NonLocalUrl);
    }

    Ok(Some(url))
}
