
use super::DirectAdapter;
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
    let version = DirectAdapter::parse_version_from_filename("tool-v1.2.3-linux-x86_64.tar.gz")
        .expect("parsed version");
    assert_eq!(version.major, 1);
    assert_eq!(version.minor, 2);
    assert_eq!(version.patch, 3);
}

#[tokio::test]
async fn get_latest_release_builds_release_from_probe_metadata() {
    let etag = "\"etag-value\"".to_string();
    let server = spawn_test_server(1, move |method, _| {
        assert_eq!(method, "HEAD");
        http_response(
            "HTTP/1.1 200 OK",
            &[
                ("Connection", "close"),
                ("Content-Length", "42"),
                ("ETag", &etag),
                ("Last-Modified", "Tue, 10 Feb 2026 15:04:05 GMT"),
            ],
            "",
        )
    });
    let adapter = DirectAdapter::new(HttpClient::new().expect("http client"));
    let release = adapter
        .get_latest_release(&format!("{server}/tool-v2.3.4.tar.gz"))
        .await
        .expect("release");

    assert_eq!(release.assets.len(), 1);
    assert_eq!(release.version.major, 2);
    assert_eq!(release.version.minor, 3);
    assert_eq!(release.version.patch, 4);
    assert!(release.name.contains("etag-value"));
}

#[tokio::test]
async fn conditional_latest_release_returns_none_on_not_modified() {
    let server = spawn_test_server(1, move |method, _| {
        assert_eq!(method, "HEAD");
        http_response("HTTP/1.1 304 Not Modified", &[("Connection", "close")], "")
    });
    let adapter = DirectAdapter::new(HttpClient::new().expect("http client"));

    let release = adapter
        .get_latest_release_if_modified_since(&server, Some(Utc::now()))
        .await
        .expect("conditional release");
    assert!(release.is_none());
}
