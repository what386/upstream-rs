use anyhow::{Context, Result, anyhow, bail};
use futures_util::StreamExt;
use reqwest::{Client, Response, StatusCode, header};
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::Instrument;

use crate::application::cancellation;
use crate::models::upstream::config::DownloadConfig;
use crate::providers::shared::http_status;

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

#[tracing::instrument(skip_all, fields(destination = %destination.display()))]
pub async fn download_file<F>(
    client: &Client,
    url: &str,
    destination: &Path,
    progress: &mut Option<F>,
    download_config: DownloadConfig,
) -> Result<()>
where
    F: FnMut(u64, u64),
{
    cancellation::check()?;
    let started = Instant::now();
    tracing::info!(source = %crate::logging::source_url(url), "Download started");

    let response = client.get(url);
    let response = tokio::select! {
        response = response.send() => response,
        _ = cancellation::cancelled() => return Err(cancellation::Cancelled.into()),
    }
    .map_err(reqwest::Error::without_url)
    .with_context(|| {
        format!(
            "Failed to download from {}",
            crate::logging::source_url(url)
        )
    })?;

    http_status::error_for_status(
        &response,
        "Download server",
        &crate::logging::source_url(url),
    )?;

    let source = crate::logging::source_url(response.url().as_str());
    let total_bytes = response.content_length().unwrap_or(0);

    let worker_count = parallel_worker_count(&response, total_bytes, download_config);
    tracing::debug!(total_bytes, worker_count, "Download strategy selected");
    let bytes = if worker_count > 1 {
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
            Ok(bytes) => bytes,
            Err(error) if error.downcast_ref::<RangeUnsupported>().is_some() => {
                tracing::debug!("Server ignored byte ranges; falling back to a single request");
                download_single_request(client, url, destination, progress).await?
            }
            Err(error) => return Err(error),
        }
    } else {
        write_single_response_atomic(response, destination, total_bytes, progress).await?
    };

    tracing::info!(%source, bytes, elapsed_ms = started.elapsed().as_millis() as u64, "Download completed");
    Ok(())
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
) -> Result<u64>
where
    F: FnMut(u64, u64),
{
    cancellation::check()?;

    let response = client.get(url).send();
    let response = tokio::select! {
        response = response => response,
        _ = cancellation::cancelled() => return Err(cancellation::Cancelled.into()),
    }
    .map_err(reqwest::Error::without_url)
    .with_context(|| {
        format!(
            "Failed to download from {}",
            crate::logging::source_url(url)
        )
    })?;

    http_status::error_for_status(
        &response,
        "Download server",
        &crate::logging::source_url(url),
    )?;

    let total_bytes = response.content_length().unwrap_or(0);
    write_single_response_atomic(response, destination, total_bytes, progress).await
}

async fn write_single_response_atomic<F>(
    response: Response,
    destination: &Path,
    total_bytes: u64,
    progress: &mut Option<F>,
) -> Result<u64>
where
    F: FnMut(u64, u64),
{
    let temp_path = temporary_destination_path(destination);
    let result = write_single_response(response, &temp_path, total_bytes, progress).await;
    let bytes = match result {
        Ok(bytes) => bytes,
        Err(error) => {
            cleanup_temp_file(&temp_path).await;
            return Err(error);
        }
    };

    if let Err(error) = move_temp_file(&temp_path, destination).await {
        cleanup_temp_file(&temp_path).await;
        return Err(error);
    }

    Ok(bytes)
}

async fn write_single_response<F>(
    response: Response,
    destination: &Path,
    total_bytes: u64,
    progress: &mut Option<F>,
) -> Result<u64>
where
    F: FnMut(u64, u64),
{
    let mut file = File::create(destination)
        .await
        .context(format!("Failed to create file at {:?}", destination))?;

    let mut stream = response.bytes_stream();
    let mut total_read: u64 = 0;

    loop {
        let chunk = tokio::select! {
            _ = cancellation::cancelled() => return Err(cancellation::Cancelled.into()),
            chunk = stream.next() => chunk,
        };

        let Some(chunk) = chunk else { break };
        cancellation::check()?;
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

    Ok(total_read)
}

async fn download_parallel<F>(
    client: &Client,
    url: &str,
    destination: &Path,
    initial_response: Response,
    total_bytes: u64,
    worker_count: usize,
    progress: &mut Option<F>,
) -> Result<u64>
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
    Ok(total_bytes)
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

    tasks.spawn(
        write_initial_range(
            initial_response,
            temp_path.to_path_buf(),
            ranges[0],
            progress_tx.clone(),
        )
        .in_current_span(),
    );

    for range in ranges.iter().copied().skip(1) {
        tasks.spawn(
            download_range(
                client.clone(),
                url.to_string(),
                temp_path.to_path_buf(),
                range,
                total_bytes,
                progress_tx.clone(),
            )
            .in_current_span(),
        );
    }

    drop(progress_tx);

    let mut completed_tasks = 0_usize;
    let mut written_total = 0_u64;
    let mut reported_total = 0_u64;
    let mut progress_closed = false;

    while completed_tasks < ranges.len() {
        tokio::select! {
            _ = cancellation::cancelled() => {
                tasks.abort_all();
                while tasks.join_next().await.is_some() {}
                return Err(cancellation::Cancelled.into());
            }
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
        cancellation::check()?;
        let chunk = tokio::select! {
            _ = cancellation::cancelled() => return Err(cancellation::Cancelled.into()),
            chunk = stream.next() => chunk,
        }
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
        .send();

    let response = tokio::select! {
        response = response => response,
        _ = cancellation::cancelled() => return Err(cancellation::Cancelled.into()),
    }
    .map_err(reqwest::Error::without_url)
    .with_context(|| {
        format!(
            "Failed to download range from {}",
            crate::logging::source_url(&url)
        )
    })?;

    if response.status() != StatusCode::PARTIAL_CONTENT {
        if let Some(message) = http_status::rate_limit_message(
            response.status(),
            response.headers(),
            "Download server",
            &crate::logging::source_url(&url),
        ) {
            bail!("{message}");
        }

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

    loop {
        let chunk = tokio::select! {
            _ = cancellation::cancelled() => return Err(cancellation::Cancelled.into()),
            chunk = stream.next() => chunk,
        };

        let Some(chunk) = chunk else { break };
        cancellation::check()?;
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
