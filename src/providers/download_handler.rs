use anyhow::{Context, Result, anyhow, bail};
use futures_util::StreamExt;
use reqwest::{Client, Response, StatusCode, header};
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::models::upstream::DownloadConfig;

#[derive(Debug, Clone, Copy)]
struct ByteRange {
    start: u64,
    end: u64,
}

impl ByteRange {
    fn len(self) -> u64 {
        self.end - self.start + 1
    }
}

#[derive(Debug)]
struct RangeUnsupported {
    status: StatusCode,
}

impl fmt::Display for RangeUnsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Server did not honor byte range request ({})",
            self.status
        )
    }
}

impl Error for RangeUnsupported {}

pub async fn download_file<F>(
    client: &Client,
    url: &str,
    destination: &Path,
    progress: &mut Option<F>,
) -> Result<()>
where
    F: FnMut(u64, u64),
{
    download_file_with_options(
        client,
        url,
        destination,
        progress,
        DownloadConfig::default(),
    )
    .await
}

pub async fn download_file_with_config<F>(
    client: &Client,
    url: &str,
    destination: &Path,
    progress: &mut Option<F>,
    download_config: DownloadConfig,
) -> Result<()>
where
    F: FnMut(u64, u64),
{
    download_file_with_options(client, url, destination, progress, download_config).await
}

async fn download_file_with_options<F>(
    client: &Client,
    url: &str,
    destination: &Path,
    progress: &mut Option<F>,
    download_config: DownloadConfig,
) -> Result<()>
where
    F: FnMut(u64, u64),
{
    let response = client
        .get(url)
        .send()
        .await
        .context(format!("Failed to download from {}", url))?;

    response
        .error_for_status_ref()
        .context("Download request failed")?;

    let total_bytes = response.content_length().unwrap_or(0);

    let worker_count = parallel_worker_count(&response, total_bytes, download_config);
    if worker_count > 1 {
        match download_parallel(
            client,
            url,
            destination,
            response,
            total_bytes,
            worker_count,
            progress,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(error) if error.downcast_ref::<RangeUnsupported>().is_some() => {
                return download_single_request(client, url, destination, progress).await;
            }
            Err(error) => return Err(error),
        }
    }

    write_single_response(response, destination, total_bytes, progress).await
}

fn parallel_worker_count(
    response: &Response,
    total_bytes: u64,
    download_config: DownloadConfig,
) -> usize {
    let supports_ranges = response
        .headers()
        .get(header::ACCEPT_RANGES)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.eq_ignore_ascii_case("bytes"))
        .unwrap_or(false);

    if supports_ranges {
        worker_count_for_size(total_bytes, download_config)
    } else {
        1
    }
}

fn worker_count_for_size(total_bytes: u64, download_config: DownloadConfig) -> usize {
    if total_bytes >= download_config.high_threshold_bytes() {
        download_config.high_threads
    } else if total_bytes >= download_config.low_threshold_bytes() {
        download_config.low_threads
    } else {
        1
    }
    .max(1)
}

async fn download_single_request<F>(
    client: &Client,
    url: &str,
    destination: &Path,
    progress: &mut Option<F>,
) -> Result<()>
where
    F: FnMut(u64, u64),
{
    let response = client
        .get(url)
        .send()
        .await
        .context(format!("Failed to download from {}", url))?;

    response
        .error_for_status_ref()
        .context("Download request failed")?;

    let total_bytes = response.content_length().unwrap_or(0);
    write_single_response(response, destination, total_bytes, progress).await
}

async fn write_single_response<F>(
    response: Response,
    destination: &Path,
    total_bytes: u64,
    progress: &mut Option<F>,
) -> Result<()>
where
    F: FnMut(u64, u64),
{
    let mut file = File::create(destination)
        .await
        .context(format!("Failed to create file at {:?}", destination))?;

    let mut stream = response.bytes_stream();
    let mut total_read: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Failed to read download chunk")?;

        file.write_all(&chunk)
            .await
            .context("Failed to write to file")?;

        total_read += chunk.len() as u64;

        if let Some(cb) = progress.as_mut() {
            cb(total_read, total_bytes);
        }
    }

    file.flush().await.context("Failed to flush file")?;

    if total_bytes > 0 && total_read != total_bytes {
        bail!(
            "Download size mismatch: expected {} bytes, got {} bytes",
            total_bytes,
            total_read
        );
    }

    Ok(())
}

async fn download_parallel<F>(
    client: &Client,
    url: &str,
    destination: &Path,
    initial_response: Response,
    total_bytes: u64,
    worker_count: usize,
    progress: &mut Option<F>,
) -> Result<()>
where
    F: FnMut(u64, u64),
{
    let ranges = split_ranges(total_bytes, worker_count);
    if ranges.len() <= 1 {
        return write_single_response(initial_response, destination, total_bytes, progress).await;
    }

    let temp_path = temporary_destination_path(destination);
    let temp_file = File::create(&temp_path)
        .await
        .context(format!("Failed to create file at {:?}", temp_path))?;
    temp_file
        .set_len(total_bytes)
        .await
        .context("Failed to preallocate download file")?;
    drop(temp_file);

    let result = run_parallel_download(
        client,
        url,
        &temp_path,
        initial_response,
        total_bytes,
        &ranges,
        progress,
    )
    .await;

    if let Err(error) = result {
        cleanup_temp_file(&temp_path).await;
        return Err(error);
    }

    move_temp_file(&temp_path, destination).await?;
    Ok(())
}

async fn run_parallel_download<F>(
    client: &Client,
    url: &str,
    temp_path: &Path,
    initial_response: Response,
    total_bytes: u64,
    ranges: &[ByteRange],
    progress: &mut Option<F>,
) -> Result<()>
where
    F: FnMut(u64, u64),
{
    let (progress_tx, mut progress_rx) = mpsc::channel(ranges.len() * 16);
    let mut tasks = JoinSet::new();

    tasks.spawn(write_initial_range(
        initial_response,
        temp_path.to_path_buf(),
        ranges[0],
        progress_tx.clone(),
    ));

    for range in ranges.iter().copied().skip(1) {
        tasks.spawn(download_range(
            client.clone(),
            url.to_string(),
            temp_path.to_path_buf(),
            range,
            total_bytes,
            progress_tx.clone(),
        ));
    }
    drop(progress_tx);

    let mut completed_tasks = 0_usize;
    let mut written_total = 0_u64;
    let mut reported_total = 0_u64;
    let mut progress_closed = false;

    while completed_tasks < ranges.len() {
        tokio::select! {
            maybe_joined = tasks.join_next() => {
                let joined = maybe_joined.ok_or_else(|| anyhow!("Parallel download worker set ended early"))?;
                match joined.context("Parallel download worker failed to join")? {
                    Ok(written) => {
                        completed_tasks += 1;
                        written_total += written;
                    }
                    Err(error) => {
                        tasks.abort_all();
                        while tasks.join_next().await.is_some() {}
                        return Err(error);
                    }
                }
            }
            maybe_delta = progress_rx.recv(), if !progress_closed => {
                match maybe_delta {
                    Some(delta) => {
                        reported_total += delta;
                        if let Some(cb) = progress.as_mut() {
                            cb(reported_total, total_bytes);
                        }
                    }
                    None => {
                        progress_closed = true;
                    }
                }
            }
        }
    }

    while let Ok(delta) = progress_rx.try_recv() {
        reported_total += delta;
        if let Some(cb) = progress.as_mut() {
            cb(reported_total, total_bytes);
        }
    }

    if written_total != total_bytes {
        bail!(
            "Download size mismatch: expected {} bytes, got {} bytes",
            total_bytes,
            written_total
        );
    }

    if reported_total != total_bytes
        && let Some(cb) = progress.as_mut()
    {
        cb(total_bytes, total_bytes);
    }

    Ok(())
}

async fn write_initial_range(
    response: Response,
    temp_path: PathBuf,
    range: ByteRange,
    progress: mpsc::Sender<u64>,
) -> Result<u64> {
    let mut file = open_range_file(&temp_path, range.start).await?;
    let mut stream = response.bytes_stream();
    let mut total_read = 0_u64;

    while total_read < range.len() {
        let chunk = stream
            .next()
            .await
            .transpose()
            .context("Failed to read download chunk")?
            .ok_or_else(|| anyhow!("Initial download stream ended before assigned range"))?;
        let remaining = (range.len() - total_read) as usize;
        let write_len = chunk.len().min(remaining);
        if write_len == 0 {
            continue;
        }

        file.write_all(&chunk[..write_len])
            .await
            .context("Failed to write to file")?;
        total_read += write_len as u64;
        let _ = progress.send(write_len as u64).await;
    }

    file.flush().await.context("Failed to flush file")?;
    Ok(total_read)
}

async fn download_range(
    client: Client,
    url: String,
    temp_path: PathBuf,
    range: ByteRange,
    total_bytes: u64,
    progress: mpsc::Sender<u64>,
) -> Result<u64> {
    let response = client
        .get(&url)
        .header(
            header::RANGE,
            format!("bytes={}-{}", range.start, range.end),
        )
        .send()
        .await
        .context(format!("Failed to download range from {}", url))?;

    if response.status() != StatusCode::PARTIAL_CONTENT {
        return Err(RangeUnsupported {
            status: response.status(),
        }
        .into());
    }

    validate_content_range(response.headers(), range, total_bytes)?;

    if let Some(content_length) = response.content_length()
        && content_length != range.len()
    {
        bail!(
            "Range size mismatch for bytes {}-{}: expected {} bytes, server reported {} bytes",
            range.start,
            range.end,
            range.len(),
            content_length
        );
    }

    let mut file = open_range_file(&temp_path, range.start).await?;
    let mut stream = response.bytes_stream();
    let mut total_read = 0_u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Failed to read download chunk")?;
        let next_total = total_read + chunk.len() as u64;
        if next_total > range.len() {
            bail!(
                "Range overflow for bytes {}-{}: received more than {} bytes",
                range.start,
                range.end,
                range.len()
            );
        }

        file.write_all(&chunk)
            .await
            .context("Failed to write to file")?;
        total_read = next_total;
        let _ = progress.send(chunk.len() as u64).await;
    }

    file.flush().await.context("Failed to flush file")?;

    if total_read != range.len() {
        bail!(
            "Range size mismatch for bytes {}-{}: expected {} bytes, got {} bytes",
            range.start,
            range.end,
            range.len(),
            total_read
        );
    }

    Ok(total_read)
}

async fn open_range_file(path: &Path, offset: u64) -> Result<File> {
    let mut file = OpenOptions::new()
        .write(true)
        .open(path)
        .await
        .context(format!("Failed to open file at {:?}", path))?;
    file.seek(SeekFrom::Start(offset))
        .await
        .context("Failed to seek in download file")?;
    Ok(file)
}

fn validate_content_range(
    headers: &header::HeaderMap,
    expected_range: ByteRange,
    expected_total: u64,
) -> Result<()> {
    let raw = headers
        .get(header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| anyhow!("Range response did not include Content-Range"))?;
    let (start, end, total) = parse_content_range(raw)
        .ok_or_else(|| anyhow!("Range response had invalid Content-Range: {}", raw))?;

    if start != expected_range.start || end != expected_range.end || total != expected_total {
        bail!(
            "Range response mismatch: expected bytes {}-{}/{}, got bytes {}-{}/{}",
            expected_range.start,
            expected_range.end,
            expected_total,
            start,
            end,
            total
        );
    }

    Ok(())
}

fn parse_content_range(value: &str) -> Option<(u64, u64, u64)> {
    let (unit, value) = value.trim().split_once(' ')?;
    if !unit.eq_ignore_ascii_case("bytes") {
        return None;
    }

    let (range, total) = value.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    Some((start.parse().ok()?, end.parse().ok()?, total.parse().ok()?))
}

fn split_ranges(total_bytes: u64, max_workers: usize) -> Vec<ByteRange> {
    if total_bytes == 0 || max_workers == 0 {
        return Vec::new();
    }

    let worker_count = max_workers.min(usize::try_from(total_bytes).unwrap_or(usize::MAX));
    let base_size = total_bytes / worker_count as u64;
    let extra_bytes = total_bytes % worker_count as u64;
    let mut start = 0_u64;
    let mut ranges = Vec::with_capacity(worker_count);

    for index in 0..worker_count {
        let len = base_size + if (index as u64) < extra_bytes { 1 } else { 0 };
        let end = start + len - 1;
        ranges.push(ByteRange { start, end });
        start = end + 1;
    }

    ranges
}

fn temporary_destination_path(destination: &Path) -> PathBuf {
    let mut file_name = destination
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("download"));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    file_name.push(format!(".upstream-part-{}-{}", std::process::id(), nanos));
    destination.with_file_name(file_name)
}

async fn cleanup_temp_file(path: &Path) {
    match fs::remove_file(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {}
    }
}

async fn move_temp_file(temp_path: &Path, destination: &Path) -> Result<()> {
    match fs::rename(temp_path, destination).await {
        Ok(()) => Ok(()),
        Err(first_error) => {
            cleanup_temp_file(destination).await;
            fs::rename(temp_path, destination).await.with_context(|| {
                format!(
                    "Failed to move downloaded file from {:?} to {:?}: {}",
                    temp_path, destination, first_error
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::download_file_with_options;
    use crate::models::upstream::DownloadConfig;
    use reqwest::Client;
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    struct TestRequest {
        method: String,
        headers: HashMap<String, String>,
    }

    impl TestRequest {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .get(&name.to_ascii_lowercase())
                .map(String::as_str)
        }
    }

    fn spawn_test_server<F>(max_requests: usize, handler: F) -> String
    where
        F: Fn(TestRequest) -> Vec<u8> + Send + 'static,
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
                let method = parts.next().unwrap_or("").to_string();

                let mut headers = HashMap::new();
                let mut line = String::new();
                loop {
                    line.clear();
                    reader.read_line(&mut line).expect("read request headers");
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some((name, value)) = line.trim_end().split_once(':') {
                        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
                    }
                }

                let response = handler(TestRequest { method, headers });
                stream.write_all(&response).expect("write response");
                stream.flush().expect("flush response");
            }
        });

        let addr = rx.recv().expect("receive server address");
        format!("http://{}", addr)
    }

    fn http_response(status_line: &str, headers: &[(&str, String)], body: &[u8]) -> Vec<u8> {
        let mut out = format!("{status_line}\r\n");
        for (k, v) in headers {
            out.push_str(&format!("{k}: {v}\r\n"));
        }
        out.push_str("Connection: close\r\n");
        out.push_str("\r\n");
        let mut bytes = out.into_bytes();
        bytes.extend_from_slice(body);
        bytes
    }

    fn body_bytes() -> Vec<u8> {
        (0..4096).map(|index| (index % 251) as u8).collect()
    }

    fn parse_range(value: &str) -> (usize, usize) {
        let range = value.strip_prefix("bytes=").expect("range prefix");
        let (start, end) = range.split_once('-').expect("range separator");
        (
            start.parse().expect("range start"),
            end.parse().expect("range end"),
        )
    }

    fn temp_file_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-download-test-{name}-{nanos}.bin"))
    }

    fn cleanup_file(path: &Path) -> io::Result<()> {
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn download_file_uses_parallel_ranges_when_server_supports_them() {
        let body = body_bytes();
        let len = body.len();
        let body_for_server = body.clone();
        let range_requests = Arc::new(Mutex::new(Vec::new()));
        let range_requests_for_server = Arc::clone(&range_requests);
        let server = spawn_test_server(4, move |request| {
            assert_eq!(request.method, "GET");
            match request.header("range") {
                Some(range_header) => {
                    range_requests_for_server
                        .lock()
                        .expect("range requests")
                        .push(range_header.to_string());
                    let (start, end) = parse_range(range_header);
                    http_response(
                        "HTTP/1.1 206 Partial Content",
                        &[
                            ("Content-Type", "application/octet-stream".to_string()),
                            ("Content-Length", (end - start + 1).to_string()),
                            ("Content-Range", format!("bytes {start}-{end}/{len}")),
                        ],
                        &body_for_server[start..=end],
                    )
                }
                None => http_response(
                    "HTTP/1.1 200 OK",
                    &[
                        ("Content-Type", "application/octet-stream".to_string()),
                        ("Content-Length", len.to_string()),
                        ("Accept-Ranges", "bytes".to_string()),
                    ],
                    &body_for_server,
                ),
            }
        });
        let client = Client::new();
        let output = temp_file_path("parallel");
        let mut progress = Vec::new();
        let mut cb = Some(|downloaded: u64, total: u64| {
            progress.push((downloaded, total));
        });

        download_file_with_options(
            &client,
            &server,
            &output,
            &mut cb,
            DownloadConfig {
                low_threshold_mb: 0,
                high_threshold_mb: 0,
                low_threads: 2,
                high_threads: 4,
            },
        )
        .await
        .expect("download file");

        assert_eq!(fs::read(&output).expect("read output file"), body);
        let mut observed_ranges = range_requests.lock().expect("range requests").clone();
        observed_ranges.sort();
        assert_eq!(
            observed_ranges.as_slice(),
            &["bytes=1024-2047", "bytes=2048-3071", "bytes=3072-4095"]
        );
        assert_eq!(
            progress.last().copied().expect("final progress"),
            (len as u64, len as u64)
        );

        cleanup_file(&output).expect("cleanup output file");
    }

    #[tokio::test]
    async fn download_file_falls_back_when_range_request_is_not_honored() {
        let body = body_bytes();
        let len = body.len();
        let body_for_server = body.clone();
        let request_log = Arc::new(Mutex::new(Vec::new()));
        let request_log_for_server = Arc::clone(&request_log);
        let server = spawn_test_server(5, move |request| {
            request_log_for_server
                .lock()
                .expect("request log")
                .push(request.header("range").map(str::to_string));
            http_response(
                "HTTP/1.1 200 OK",
                &[
                    ("Content-Type", "application/octet-stream".to_string()),
                    ("Content-Length", len.to_string()),
                    ("Accept-Ranges", "bytes".to_string()),
                ],
                &body_for_server,
            )
        });
        let client = Client::new();
        let output = temp_file_path("fallback");
        let mut progress = Vec::new();
        let mut cb = Some(|downloaded: u64, total: u64| {
            progress.push((downloaded, total));
        });

        download_file_with_options(
            &client,
            &server,
            &output,
            &mut cb,
            DownloadConfig {
                low_threshold_mb: 0,
                high_threshold_mb: 0,
                low_threads: 2,
                high_threads: 4,
            },
        )
        .await
        .expect("download file");

        assert_eq!(fs::read(&output).expect("read output file"), body);
        let log = request_log.lock().expect("request log");
        assert!(log.iter().any(Option::is_some));
        assert!(matches!(log.last(), Some(None)));
        assert_eq!(
            progress.last().copied().expect("final progress"),
            (len as u64, len as u64)
        );

        cleanup_file(&output).expect("cleanup output file");
    }
}
