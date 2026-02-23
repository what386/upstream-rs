use super::WebScraperAdapter;
use crate::providers::http::HttpClient;
use chrono::Utc;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;

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

#[test]
fn parse_version_from_filename_extracts_semver_triplet() {
    let version = WebScraperAdapter::parse_version_from_filename("tool-v1.4.9-linux.tar.gz")
        .expect("parsed version");
    assert_eq!(version.major, 1);
    assert_eq!(version.minor, 4);
    assert_eq!(version.patch, 9);
}

#[tokio::test]
async fn get_latest_release_selects_assets_for_latest_detected_version() {
    let html = r#"
            <html><body>
                <a href="/tool-v1.9.0-linux.tar.gz">old</a>
                <a href="/tool-v1.10.0-linux.tar.gz">new</a>
                <a href="/tool-v1.10.0-linux.sha256">checksum</a>
            </body></html>
        "#
    .to_string();
    let html_len = html.len().to_string();
    let html_for_server = html.clone();
    let server = spawn_test_server(1, move |method, _| {
        assert_eq!(method, "GET");
        http_response(
            "HTTP/1.1 200 OK",
            &[
                ("Connection", "close"),
                ("Content-Type", "text/html"),
                ("Content-Length", &html_len),
            ],
            &html_for_server,
        )
    });

    let adapter = WebScraperAdapter::new(HttpClient::new().expect("http client"));
    let release = adapter
        .get_latest_release(&server)
        .await
        .expect("latest release");

    assert_eq!(release.version.major, 1);
    assert_eq!(release.version.minor, 10);
    assert_eq!(release.version.patch, 0);
    assert_eq!(release.assets.len(), 1);
    assert!(release.assets[0].name.contains("1.10.0"));
}

#[tokio::test]
async fn conditional_latest_release_returns_none_on_not_modified() {
    let server = spawn_test_server(1, move |method, _| {
        assert_eq!(method, "GET");
        http_response("HTTP/1.1 304 Not Modified", &[("Connection", "close")], "")
    });
    let adapter = WebScraperAdapter::new(HttpClient::new().expect("http client"));
    let release = adapter
        .get_latest_release_if_modified_since(&server, Some(Utc::now()))
        .await
        .expect("conditional release");
    assert!(release.is_none());
}
