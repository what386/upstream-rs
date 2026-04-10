use anyhow::{Context, Result, bail};
use reqwest::Client;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

pub async fn download_file<F>(
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

    let mut file = File::create(destination)
        .await
        .context(format!("Failed to create file at {:?}", destination))?;

    let mut stream = response.bytes_stream();
    let mut total_read: u64 = 0;

    use futures_util::StreamExt;
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
