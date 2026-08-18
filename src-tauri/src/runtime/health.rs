use std::time::Duration;

use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use url::Url;

const MAX_STATUS_LINE_BYTES: usize = 1024;

#[derive(Debug, Error)]
pub enum HealthError {
    #[error("DSH 健康检查地址不是受信任的本机 HTTP 地址")]
    InvalidUrl,
    #[error("DSH HTTP 服务在限定时间内没有响应")]
    Timeout,
    #[error("无法连接 DSH HTTP 服务：{0}")]
    Connect(std::io::Error),
    #[error("DSH HTTP 健康请求失败：{0}")]
    Io(std::io::Error),
    #[error("DSH HTTP 服务返回了无效响应")]
    InvalidResponse,
    #[error("DSH HTTP 服务尚未就绪（状态 {0}）")]
    UnhealthyStatus(u16),
}

/// 只探测已经过就绪地址校验的本机 HTTP 服务，不跟随重定向或读取页面正文。
pub async fn probe_http(url: &Url, timeout: Duration) -> Result<(), HealthError> {
    validate_url(url)?;
    tokio::time::timeout(timeout, probe_once(url))
        .await
        .map_err(|_| HealthError::Timeout)?
}

fn validate_url(url: &Url) -> Result<(), HealthError> {
    let valid = url.scheme() == "http"
        && url.host_str() == Some("127.0.0.1")
        && url.port().is_some_and(|port| port > 0)
        && url.username().is_empty()
        && url.password().is_none();
    valid.then_some(()).ok_or(HealthError::InvalidUrl)
}

async fn probe_once(url: &Url) -> Result<(), HealthError> {
    let port = url.port().ok_or(HealthError::InvalidUrl)?;
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .map_err(HealthError::Connect)?;
    let target = match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_string(),
    };
    let request =
        format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(HealthError::Io)?;

    let mut response = Vec::with_capacity(128);
    let mut buffer = [0_u8; 128];
    while response.len() < MAX_STATUS_LINE_BYTES {
        let read = stream.read(&mut buffer).await.map_err(HealthError::Io)?;
        if read == 0 {
            break;
        }
        let remaining = MAX_STATUS_LINE_BYTES - response.len();
        response.extend_from_slice(&buffer[..read.min(remaining)]);
        if response.windows(2).any(|window| window == b"\r\n") {
            break;
        }
    }

    let status_line_end = response
        .windows(2)
        .position(|window| window == b"\r\n")
        .ok_or(HealthError::InvalidResponse)?;
    let status_line = std::str::from_utf8(&response[..status_line_end])
        .map_err(|_| HealthError::InvalidResponse)?;
    let mut parts = status_line.split_whitespace();
    let protocol = parts.next().ok_or(HealthError::InvalidResponse)?;
    let status = parts
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or(HealthError::InvalidResponse)?;
    if !protocol.starts_with("HTTP/1.") && !protocol.starts_with("HTTP/2") {
        return Err(HealthError::InvalidResponse);
    }
    if (200..400).contains(&status) {
        Ok(())
    } else {
        Err(HealthError::UnhealthyStatus(status))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    use url::Url;

    use super::{probe_http, HealthError};

    async fn local_server(response: &'static [u8]) -> (Url, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("测试服务应能监听本机端口");
        let address = listener.local_addr().expect("测试服务应有地址");
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("应收到健康请求");
            let mut request = [0_u8; 512];
            let _ = stream.read(&mut request).await.expect("应能读取健康请求");
            stream.write_all(response).await.expect("应能写入测试响应");
        });
        (
            Url::parse(&format!("http://127.0.0.1:{}/", address.port())).expect("测试地址应有效"),
            task,
        )
    }

    #[tokio::test]
    async fn probe_accepts_a_successful_local_http_response() {
        let (url, server) = local_server(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;

        probe_http(&url, Duration::from_secs(1))
            .await
            .expect("成功响应应通过健康检查");
        server.await.expect("测试服务不应崩溃");
    }

    #[tokio::test]
    async fn probe_rejects_an_unhealthy_http_status() {
        let (url, server) =
            local_server(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n").await;

        let error = probe_http(&url, Duration::from_secs(1))
            .await
            .expect_err("503 不应被视为就绪");

        assert!(matches!(error, HealthError::UnhealthyStatus(503)));
        server.await.expect("测试服务不应崩溃");
    }

    #[tokio::test]
    async fn probe_times_out_when_the_server_never_responds() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("测试服务应能监听本机端口");
        let address = listener.local_addr().expect("测试服务应有地址");
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("应收到健康请求");
            tokio::time::sleep(Duration::from_secs(1)).await;
        });
        let url =
            Url::parse(&format!("http://127.0.0.1:{}/", address.port())).expect("测试地址应有效");

        let error = probe_http(&url, Duration::from_millis(20))
            .await
            .expect_err("无响应服务必须超时");

        assert!(matches!(error, HealthError::Timeout));
        server.abort();
    }

    #[tokio::test]
    async fn probe_rejects_a_non_local_url_before_connecting() {
        let url = Url::parse("https://example.com/").expect("测试地址应有效");

        let error = probe_http(&url, Duration::from_secs(1))
            .await
            .expect_err("健康检查不得访问外部地址");

        assert!(matches!(error, HealthError::InvalidUrl));
    }
}
