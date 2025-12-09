use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::fs::File;

use flate2::read::GzDecoder;
use bzip2::read::BzDecoder;

use tar::Archive;
use zip::ZipArchive;

/// Supports:
/// - .zip
/// - .gz (single file or .tar.gz)
/// - .bz2 (single file or .tar.bz2)
/// - .tar

/// Decompress a file into the output folder.
pub fn decompress(input: impl AsRef<Path>, output: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let input = input.as_ref();
    let output = output.as_ref();
    std::fs::create_dir_all(output)?;

    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Handle multi-part extensions (.tar.gz, .tar.bz2)
    let file_name = input.file_name().unwrap().to_string_lossy();
    let name = file_name.to_lowercase();

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return decompress_tar_gz(input, output);
    }

    if name.ends_with(".tar.bz2") || name.ends_with(".tbz") {
        return decompress_tar_bz2(input, output);
    }

    match ext.as_str() {
        "zip" => decompress_zip(input, output),
        "gz" => decompress_gz_single(input, output),
        "bz2" => decompress_bz2_single(input, output),
        "tar" => unpack_tar(input, output),
        _ => Err(anyhow!("Unsupported format: {}", input.display())),
    }
}

// ---------------- ZIP ----------------

fn decompress_zip(input: &Path, output: &Path) -> Result<Vec<PathBuf>> {
    let file = File::open(input)?;
    let mut archive = ZipArchive::new(file)?;

    let mut files = Vec::new();

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
            files.push(out_path);
        }
    }

    Ok(files)
}

// ---------------- TAR ----------------

fn unpack_tar(input: &Path, output: &Path) -> Result<Vec<PathBuf>> {
    let file = File::open(input)?;
    let mut archive = Archive::new(file);
    let mut files = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = output.join(entry.path()?);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&path)?;
        files.push(path);
    }

    Ok(files)
}

// ---------------- GZIP ----------------

fn decompress_tar_gz(input: &Path, output: &Path) -> Result<Vec<PathBuf>> {
    let file = File::open(input)?;
    let tar = GzDecoder::new(file);
    let mut archive = Archive::new(tar);

    let mut files = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = output.join(entry.path()?);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&path)?;
        files.push(path);
    }

    Ok(files)
}

fn decompress_gz_single(input: &Path, output: &Path) -> Result<Vec<PathBuf>> {
    let file = File::open(input)?;
    let mut decoder = GzDecoder::new(file);

    let out_name = input
        .file_stem()
        .ok_or_else(|| anyhow!("Cannot derive output name"))?;
    let out_path = output.join(out_name);

    let mut out = File::create(&out_path)?;
    std::io::copy(&mut decoder, &mut out)?;

    Ok(vec![out_path])
}

// ---------------- BZIP2 ----------------

fn decompress_tar_bz2(input: &Path, output: &Path) -> Result<Vec<PathBuf>> {
    let file = File::open(input)?;
    let tar = BzDecoder::new(file);
    let mut archive = Archive::new(tar);

    let mut files = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = output.join(entry.path()?);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&path)?;
        files.push(path);
    }

    Ok(files)
}

fn decompress_bz2_single(input: &Path, output: &Path) -> Result<Vec<PathBuf>> {
    let file = File::open(input)?;
    let mut decoder = BzDecoder::new(file);

    let out_name = input
        .file_stem()
        .ok_or_else(|| anyhow!("Cannot derive output name"))?;
    let out_path = output.join(out_name);

    let mut out = File::create(&out_path)?;
    std::io::copy(&mut decoder, &mut out)?;

    Ok(vec![out_path])
}
