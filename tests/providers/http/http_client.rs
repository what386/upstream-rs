use super::{ConditionalDiscoveryResult, ConditionalProbeResult, HttpClient};
use chrono::Utc;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

fn spawn_test_server<F>(max_requests: usize, handler: F) -> String
where
    F: Fn(&str, &str) -> String + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let addr = listener.local_addr().expect("resolve local addr");
        tx.send(addr).expect("send test server addr");

        for _ in 0..max_requests {
            let (mut stream, _) = listener.accept().expect("accept request");
            let cloned = stream.try_clone().expect("clone stream");
            let mut reader = BufReader::new(cloned);

            let mut request_line = String::new();
            reader
                .read_line(&mut request_line)
                .expect("read request line");
            let mut parts = request_line.split_whitespace();
            let method = parts.next().unwrap_or("");
            let path = parts.next().unwrap_or("/");

            let mut line = String::new();
            loop {
                line.clear();
                reader.read_line(&mut line).expect("read request headers");
                if line == "\r\n" || line.is_empty() {
                    break;
                }
            }

            let response = handler(method, path);
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
        }
    });

    let addr = rx.recv().expect("receive server address");
    format!("http://{}", addr)
}

fn http_response(status_line: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut out = format!("{status_line}\r\n");
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str("\r\n");
    out.push_str(body);
    out
}

fn temp_file_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("upstream-http-test-{name}-{nanos}.bin"))
}

fn cleanup_file(path: &PathBuf) -> io::Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[test]
fn normalize_url_and_file_name_from_url_behave_as_expected() {
    assert_eq!(
        HttpClient::normalize_url("example.com/a"),
        "https://example.com/a"
    );
    assert_eq!(
        HttpClient::normalize_url("http://example.com/a"),
        "http://example.com/a"
    );

    assert_eq!(
        HttpClient::file_name_from_url("https://x.invalid/path/tool.tar.gz?x=1#frag"),
        "tool.tar.gz"
    );
    assert_eq!(
        HttpClient::file_name_from_url("https://x.invalid/path/"),
        "download.bin"
    );
}

#[tokio::test]
async fn discover_assets_extracts_and_filters_html_links() {
    let html = r##"
            <html><body>
                <a href="tool-v1.2.3-linux.tar.gz">main</a>
                <a href="/downloads/tool-v1.2.3-linux.tar.gz">duplicate</a>
                <a href="tool-v1.2.3.sha256">checksum</a>
                <a href="mailto:test@example.com">mail</a>
                <a href="#anchor">anchor</a>
                <a href="https://example.invalid/tool-v1.2.3-macos.zip">mac</a>
            </body></html>
        "##;
    let body = html.to_string();
    let server = spawn_test_server(1, move |_, _| {
        http_response(
            "HTTP/1.1 200 OK",
            &[
                ("Content-Type", "text/html"),
                ("Content-Length", &body.len().to_string()),
                ("Connection", "close"),
            ],
            &body,
        )
    });
    let client = HttpClient::new().expect("client");

    let result = client
        .discover_assets_if_modified_since(&server, None)
        .await
        .expect("discover assets");

    match result {
        ConditionalDiscoveryResult::NotModified => panic!("unexpected not modified"),
        ConditionalDiscoveryResult::Assets(assets) => {
            assert_eq!(assets.len(), 3);
            assert!(
                assets
                    .iter()
                    .any(|a| a.name.ends_with("tool-v1.2.3-linux.tar.gz"))
            );
            assert!(assets.iter().all(|a| !a.name.ends_with(".sha256")));
        }
    }
}

#[tokio::test]
async fn probe_asset_if_modified_since_returns_not_modified_on_304() {
    let server = spawn_test_server(1, move |method, _| {
        assert_eq!(method, "HEAD");
        http_response("HTTP/1.1 304 Not Modified", &[("Connection", "close")], "")
    });
    let client = HttpClient::new().expect("client");

    let result = client
        .probe_asset_if_modified_since(&server, Some(Utc::now()))
        .await
        .expect("probe");
    assert!(matches!(result, ConditionalProbeResult::NotModified));
}

#[tokio::test]
async fn probe_asset_if_modified_since_falls_back_to_get_on_405_head() {
    let last_modified = "Tue, 10 Feb 2026 15:04:05 GMT".to_string();
    let etag = "\"abc123\"".to_string();
    let server = spawn_test_server(2, move |method, _| match method {
        "HEAD" => http_response(
            "HTTP/1.1 405 Method Not Allowed",
            &[("Connection", "close"), ("Content-Length", "0")],
            "",
        ),
        "GET" => http_response(
            "HTTP/1.1 200 OK",
            &[
                ("Connection", "close"),
                ("Content-Length", "11"),
                ("Last-Modified", &last_modified),
                ("ETag", &etag),
            ],
            "hello world",
        ),
        _ => http_response(
            "HTTP/1.1 500 Internal Server Error",
            &[("Connection", "close"), ("Content-Length", "0")],
            "",
        ),
    });
    let client = HttpClient::new().expect("client");

    let result = client
        .probe_asset_if_modified_since(&format!("{server}/tool-v2.3.4.tar.gz"), None)
        .await
        .expect("probe fallback");

    match result {
        ConditionalProbeResult::NotModified => panic!("unexpected not modified"),
        ConditionalProbeResult::Asset(asset) => {
            assert_eq!(asset.size, 11);
            assert_eq!(asset.etag.as_deref(), Some("abc123"));
            assert!(asset.last_modified.is_some());
            assert_eq!(asset.name, "tool-v2.3.4.tar.gz");
        }
    }
}

#[tokio::test]
async fn download_file_writes_bytes_and_reports_progress() {
    let body = "stream-body-data".to_string();
    let len = body.len().to_string();
    let body_for_server = body.clone();
    let server = spawn_test_server(1, move |method, _| {
        assert_eq!(method, "GET");
        http_response(
            "HTTP/1.1 200 OK",
            &[
                ("Connection", "close"),
                ("Content-Type", "application/octet-stream"),
                ("Content-Length", &len),
            ],
            &body_for_server,
        )
    });
    let client = HttpClient::new().expect("client");
    let output = temp_file_path("download");
    let mut progress = Vec::new();
    let mut cb = Some(|downloaded: u64, total: u64| {
        progress.push((downloaded, total));
    });

    client
        .download_file(&server, &output, &mut cb)
        .await
        .expect("download file");

    assert_eq!(fs::read_to_string(&output).expect("read output file"), body);
    assert!(!progress.is_empty());
    assert_eq!(
        progress.last().copied().expect("final progress"),
        (body.len() as u64, body.len() as u64)
    );

    cleanup_file(&output).expect("cleanup output file");
}
