use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use reqwest::{header::HeaderMap, Client};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Mutex,
};
use url::Url;

use super::{install_crypto_provider, DownloadPolicy};
use crate::updater::{
    cache::{DownloadCache, DownloadIdentity},
    transfer::download_url_to_cache_with_policy,
};

fn test_directory(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "dsh-desktop-downloader-{name}-{}",
        std::process::id()
    ))
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 512];
    loop {
        let read = stream.read(&mut buffer).await.expect("应读取请求");
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8(request).expect("请求应为UTF-8")
}

#[tokio::test]
async fn stalled_transfer_resumes_from_the_persisted_file_offset() {
    install_crypto_provider();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("测试服务应能监听");
    let address = listener.local_addr().expect("测试服务应有地址");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let server_requests = requests.clone();
    let server = tokio::spawn(async move {
        let (mut first, _) = listener.accept().await.expect("应收到首次请求");
        server_requests
            .lock()
            .await
            .push(read_request(&mut first).await);
        first
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\npart")
            .await
            .expect("应写入部分响应");
        tokio::time::sleep(Duration::from_millis(80)).await;
        drop(first);

        let (mut second, _) = listener.accept().await.expect("应收到续传请求");
        server_requests
            .lock()
            .await
            .push(read_request(&mut second).await);
        second
            .write_all(
                b"HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\nContent-Range: bytes 4-7/8\r\n\r\ndone",
            )
            .await
            .expect("应写入续传响应");
    });

    let directory = test_directory("stall-resume");
    let _ = fs::remove_dir_all(&directory);
    let url = format!("http://{address}/package");
    let mut cache = DownloadCache::prepare(
        &directory,
        DownloadIdentity {
            version: "0.1.3".to_string(),
            url: url.clone(),
            signature: "test-signature".to_string(),
        },
        1024,
    )
    .expect("应准备缓存");
    let retry_count = Arc::new(AtomicUsize::new(0));
    let retry_counter = retry_count.clone();

    download_url_to_cache_with_policy(
        &Client::new(),
        Url::parse(&url).expect("地址应有效"),
        HeaderMap::new(),
        &mut cache,
        DownloadPolicy::for_test(2, Duration::from_millis(20)),
        |_, _| {},
        move |_, _, _, _, _| {
            retry_counter.fetch_add(1, Ordering::Relaxed);
        },
    )
    .await
    .expect("断流后应续传成功");

    server.await.expect("测试服务不应崩溃");
    assert_eq!(
        fs::read(cache.part_path()).expect("缓存应可读"),
        b"partdone"
    );
    let requests = requests.lock().await;
    assert!(!requests[0].contains("Range:"));
    assert!(requests[1].to_ascii_lowercase().contains("range: bytes=4-"));
    assert_eq!(retry_count.load(Ordering::Relaxed), 1);
    fs::remove_dir_all(directory).expect("应清理测试目录");
}

#[tokio::test]
async fn server_ignoring_range_replaces_the_stale_partial_file() {
    install_crypto_provider();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("测试服务应能监听");
    let address = listener.local_addr().expect("测试服务应有地址");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("应收到续传请求");
        let request = read_request(&mut stream).await;
        assert!(request.to_ascii_lowercase().contains("range: bytes=3-"));
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\nnew-data")
            .await
            .expect("应写入完整响应");
    });

    let directory = test_directory("range-ignored");
    let _ = fs::remove_dir_all(&directory);
    let url = format!("http://{address}/package");
    let mut cache = DownloadCache::prepare(
        &directory,
        DownloadIdentity {
            version: "0.1.3".to_string(),
            url: url.clone(),
            signature: "test-signature".to_string(),
        },
        1024,
    )
    .expect("应准备缓存");
    fs::write(cache.part_path(), b"old").expect("应写入旧缓存");
    cache.set_downloaded_len(3);
    cache.set_expected_total(Some(8)).expect("应保存旧总大小");

    download_url_to_cache_with_policy(
        &Client::new(),
        Url::parse(&url).expect("地址应有效"),
        HeaderMap::new(),
        &mut cache,
        DownloadPolicy::for_test(1, Duration::from_secs(1)),
        |_, _| {},
        |_, _, _, _, _| {},
    )
    .await
    .expect("200完整响应应安全替换旧缓存");

    server.await.expect("测试服务不应崩溃");
    assert_eq!(
        fs::read(cache.part_path()).expect("缓存应可读"),
        b"new-data"
    );
    fs::remove_dir_all(directory).expect("应清理测试目录");
}

#[tokio::test]
async fn mismatched_content_range_is_rejected_without_appending_data() {
    install_crypto_provider();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("测试服务应能监听");
    let address = listener.local_addr().expect("测试服务应有地址");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("应收到续传请求");
        let _ = read_request(&mut stream).await;
        stream
            .write_all(
                b"HTTP/1.1 206 Partial Content\r\nContent-Length: 5\r\nContent-Range: bytes 3-7/8\r\n\r\nwrong",
            )
            .await
            .expect("应写入错误范围响应");
    });

    let directory = test_directory("range-mismatch");
    let _ = fs::remove_dir_all(&directory);
    let url = format!("http://{address}/package");
    let mut cache = DownloadCache::prepare(
        &directory,
        DownloadIdentity {
            version: "0.1.3".to_string(),
            url: url.clone(),
            signature: "test-signature".to_string(),
        },
        1024,
    )
    .expect("应准备缓存");
    fs::write(cache.part_path(), b"part").expect("应写入部分缓存");
    cache.set_downloaded_len(4);
    cache.set_expected_total(Some(8)).expect("应保存总大小");

    let error = download_url_to_cache_with_policy(
        &Client::new(),
        Url::parse(&url).expect("地址应有效"),
        HeaderMap::new(),
        &mut cache,
        DownloadPolicy::for_test(1, Duration::from_secs(1)),
        |_, _| {},
        |_, _, _, _, _| {},
    )
    .await
    .expect_err("错误续传偏移必须失败");

    server.await.expect("测试服务不应崩溃");
    assert!(error.to_string().contains("期望从 4 续传"));
    assert_eq!(fs::read(cache.part_path()).expect("缓存应可读"), b"part");
    fs::remove_dir_all(directory).expect("应清理测试目录");
}
