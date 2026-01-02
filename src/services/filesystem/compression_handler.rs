use anyhow::{Result, anyhow};
use std::fs::File;
use std::path::{Path, PathBuf};
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use tar::Archive;
use zip::ZipArchive;

/// Decompress a file into the output folder and return the root path extracted.
///
/// Always returns a directory named after the archive (without extensions).
/// If the archive contains a single root directory, it's unwrapped.
/// If the archive contains multiple items at root, they're wrapped in the archive-named directory.
pub fn decompress(input: &Path, output: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(output)?;

    // Create a temp extraction directory
    let temp_extract = output.join("temp_extract");
    std::fs::create_dir_all(&temp_extract)?;

    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let file_name = input.file_name().unwrap().to_string_lossy();
    let name = file_name.to_lowercase();

    // Extract to temp directory first
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        decompress_tar_gz(input, &temp_extract)?;
    } else if name.ends_with(".tar.bz2") || name.ends_with(".tbz") {
        decompress_tar_bz2(input, &temp_extract)?;
    } else {
        match ext.as_str() {
            "zip" => decompress_zip(input, &temp_extract)?,
            "gz" => {
                // Single .gz file (not .tar.gz)
                std::fs::remove_dir_all(&temp_extract)?;
                return decompress_gz_single(input, output);
            }
            "bz2" => {
                // Single .bz2 file (not .tar.bz2)
                std::fs::remove_dir_all(&temp_extract)?;
                return decompress_bz2_single(input, output);
            }
            "tar" => unpack_tar(input, &temp_extract)?,
            _ => {
                std::fs::remove_dir_all(&temp_extract)?;
                return Err(anyhow!("Unsupported format: {}", input.display()));
            }
        };
    }

    // Determine the final directory name from the archive filename
    let archive_name = get_archive_stem(input);
    let final_dir = output.join(&archive_name);

    // Normalize the extracted content
    normalize_extraction(&temp_extract, &final_dir)?;

    // Clean up temp directory
    let _ = std::fs::remove_dir_all(&temp_extract);

    Ok(final_dir)
}

/// Get the archive name without extensions (.tar.gz, .tar.bz2, etc.)
fn get_archive_stem(path: &Path) -> String {
    let name = path.file_name().unwrap().to_string_lossy();

    // Remove known multi-part extensions
    let stripped = name
        .strip_suffix(".tar.gz").or_else(|| name.strip_suffix(".tgz"))
        .or_else(|| name.strip_suffix(".tar.bz2")).or_else(|| name.strip_suffix(".tbz"))
        .or_else(|| name.strip_suffix(".tar.xz"))
        .unwrap_or(&name);

    // If it still ends with .tar, remove that too
    let stripped = stripped.strip_suffix(".tar").unwrap_or(stripped);

    // Remove any remaining single extension
    if let Some(pos) = stripped.rfind('.') {
        stripped[..pos].to_string()
    } else {
        stripped.to_string()
    }
}

/// Normalize extraction: unwrap single root directory or move all contents
fn normalize_extraction(temp_dir: &Path, final_dir: &Path) -> Result<()> {
    // Get all items in the temp extraction directory
    let entries: Vec<_> = std::fs::read_dir(temp_dir)?
        .filter_map(|e| e.ok())
        .collect();

    if entries.is_empty() {
        return Err(anyhow!("Archive is empty"));
    }

    // If there's exactly one item and it's a directory, unwrap it
    if entries.len() == 1 {
        let entry = &entries[0];
        let path = entry.path();

        if path.is_dir() {
            // Unwrap: move the contents of this single directory to final_dir
            std::fs::rename(&path, final_dir)?;
            return Ok(());
        }
    }

    // Multiple items or single file: move everything to final_dir
    std::fs::create_dir_all(final_dir)?;
    for entry in entries {
        let src = entry.path();
        let dest = final_dir.join(entry.file_name());
        std::fs::rename(&src, &dest)?;
    }

    Ok(())
}

// ---------------- ZIP ----------------
fn decompress_zip(input: &Path, output: &Path) -> Result<()> {
    let file = File::open(input)?;
    let mut archive = ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let out_path = output.join(file.name());

        if file.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = File::create(&out_path)?;
            std::io::copy(&mut file, &mut out)?;
        }
    }
    Ok(())
}

// ---------------- TAR ----------------
fn unpack_tar(input: &Path, output: &Path) -> Result<()> {
    let file = File::open(input)?;
    let mut archive = Archive::new(file);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = output.join(entry.path()?);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&path)?;
    }
    Ok(())
}

// ---------------- GZIP ----------------
fn decompress_tar_gz(input: &Path, output: &Path) -> Result<()> {
    let file = File::open(input)?;
    let tar = GzDecoder::new(file);
    let mut archive = Archive::new(tar);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = output.join(entry.path()?);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&path)?;
    }
    Ok(())
}

fn decompress_gz_single(input: &Path, output: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let mut decoder = GzDecoder::new(file);
    let out_name = input
        .file_stem()
        .ok_or_else(|| anyhow!("Cannot derive output name"))?;
    let out_path = output.join(out_name);
    let mut out = File::create(&out_path)?;
    std::io::copy(&mut decoder, &mut out)?;
    Ok(out_path)
}

// ---------------- BZIP2 ----------------
fn decompress_tar_bz2(input: &Path, output: &Path) -> Result<()> {
    let file = File::open(input)?;
    let tar = BzDecoder::new(file);
    let mut archive = Archive::new(tar);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = output.join(entry.path()?);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&path)?;
    }
    Ok(())
}

fn decompress_bz2_single(input: &Path, output: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let mut decoder = BzDecoder::new(file);
    let out_name = input
        .file_stem()
        .ok_or_else(|| anyhow!("Cannot derive output name"))?;
    let out_path = output.join(out_name);
    let mut out = File::create(&out_path)?;
    std::io::copy(&mut decoder, &mut out)?;
    Ok(out_path)
}
