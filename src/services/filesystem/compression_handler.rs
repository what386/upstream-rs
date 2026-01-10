use anyhow::{Result, anyhow};
use std::fs::File;
use std::path::{Path, PathBuf};
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use xz2::read::XzDecoder;
use tar::Archive;
use zip::ZipArchive;

/// Decompress a file into the output folder and return the root path extracted.
///
/// - Single files return the file path
/// - Archives return a directory named after the archive file
/// - Single-directory archives are automatically flattened
pub fn decompress(input: &Path, output: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(output)?;

    // Create a subdirectory named after the input file (removing extensions)
    // First remove the extension (.gz, .bz2, .zip, etc.)
    let without_ext = input
        .file_stem()
        .ok_or_else(|| anyhow!("Cannot derive archive name"))?;

    // If it ends with .tar, remove that too
    let archive_name = Path::new(without_ext)
        .file_stem()
        .filter(|_| without_ext.to_string_lossy().ends_with(".tar"))
        .unwrap_or(without_ext);

    let extract_dir = output.join(archive_name);
    std::fs::create_dir_all(&extract_dir)?;

    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let file_name = input.file_name().unwrap().to_string_lossy();
    let name = file_name.to_lowercase();

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return decompress_tar_gz(input, &extract_dir);
    }
    if name.ends_with(".tar.bz2") || name.ends_with(".tbz") {
        return decompress_tar_bz2(input, &extract_dir);
    }
    if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        return decompress_tar_xz(input, &extract_dir);
    }

    match ext.as_str() {
        "zip" => decompress_zip(input, &extract_dir),
        "gz" => decompress_gz_single(input, &extract_dir),
        "bz2" => decompress_bz2_single(input, &extract_dir),
        "xz" => decompress_xz_single(input, &extract_dir),
        "tar" => unpack_tar(input, &extract_dir),
        _ => Err(anyhow!("Unsupported format: {}", input.display())),
    }
}

// ---------------- ZIP ----------------
fn decompress_zip(input: &Path, extract_dir: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let mut archive = ZipArchive::new(file)?;
    let mut paths = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let out_path = extract_dir.join(file.name());
        if file.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = File::create(&out_path)?;
            std::io::copy(&mut file, &mut out)?;
            paths.push(out_path);
        }
    }
    common_root(&paths, extract_dir)
}

// ---------------- TAR ----------------
fn unpack_tar(input: &Path, extract_dir: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let mut archive = Archive::new(file);
    let mut paths = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = extract_dir.join(entry.path()?);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&path)?;
        paths.push(path);
    }
    common_root(&paths, extract_dir)
}

// ---------------- XZ ----------------
fn decompress_tar_xz(input: &Path, extract_dir: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let tar = XzDecoder::new(file);
    let mut archive = Archive::new(tar);
    let mut paths = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = extract_dir.join(entry.path()?);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&path)?;
        paths.push(path);
    }
    common_root(&paths, extract_dir)
}

fn decompress_xz_single(input: &Path, extract_dir: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let mut decoder = XzDecoder::new(file);
    let out_name = input
        .file_stem()
        .ok_or_else(|| anyhow!("Cannot derive output name"))?;
    let out_path = extract_dir.join(out_name);
    let mut out = File::create(&out_path)?;
    std::io::copy(&mut decoder, &mut out)?;
    Ok(out_path)
}

// ---------------- GZIP ----------------
fn decompress_tar_gz(input: &Path, extract_dir: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let tar = GzDecoder::new(file);
    let mut archive = Archive::new(tar);
    let mut paths = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = extract_dir.join(entry.path()?);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&path)?;
        paths.push(path);
    }
    common_root(&paths, extract_dir)
}

fn decompress_gz_single(input: &Path, extract_dir: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let mut decoder = GzDecoder::new(file);
    let out_name = input
        .file_stem()
        .ok_or_else(|| anyhow!("Cannot derive output name"))?;
    let out_path = extract_dir.join(out_name);
    let mut out = File::create(&out_path)?;
    std::io::copy(&mut decoder, &mut out)?;
    Ok(out_path)
}

// ---------------- BZIP2 ----------------
fn decompress_tar_bz2(input: &Path, extract_dir: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let tar = BzDecoder::new(file);
    let mut archive = Archive::new(tar);
    let mut paths = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = extract_dir.join(entry.path()?);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&path)?;
        paths.push(path);
    }
    common_root(&paths, extract_dir)
}

fn decompress_bz2_single(input: &Path, extract_dir: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let mut decoder = BzDecoder::new(file);
    let out_name = input
        .file_stem()
        .ok_or_else(|| anyhow!("Cannot derive output name"))?;
    let out_path = extract_dir.join(out_name);
    let mut out = File::create(&out_path)?;
    std::io::copy(&mut decoder, &mut out)?;
    Ok(out_path)
}

/// Determine the root directory of extracted paths and flatten if needed
/// If archive contains a single top-level directory, move its contents up and return extract_dir
/// Otherwise, return extract_dir as-is
fn common_root(paths: &[PathBuf], extract_dir: &Path) -> Result<PathBuf> {
    if paths.is_empty() {
        return Ok(extract_dir.to_path_buf());
    }

    // Collect all top-level entries (immediate children of extract_dir)
    let mut top_level_entries: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    for path in paths {
        if let Ok(relative) = path.strip_prefix(extract_dir) {
            if let Some(first_component) = relative.components().next() {
                top_level_entries.insert(extract_dir.join(first_component.as_os_str()));
            }
        }
    }

    // If there's exactly one top-level entry and it's a directory, flatten it
    if top_level_entries.len() == 1 {
        let single_dir = top_level_entries.into_iter().next().unwrap();
        if single_dir.is_dir() {
            // Move contents of single_dir up to extract_dir
            for entry in std::fs::read_dir(&single_dir)? {
                let entry = entry?;
                let dest = extract_dir.join(entry.file_name());
                std::fs::rename(entry.path(), dest)?;
            }
            // Remove the now-empty single directory
            std::fs::remove_dir(&single_dir)?;
        }
    }

    Ok(extract_dir.to_path_buf())
}
