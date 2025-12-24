use anyhow::{Result, anyhow};
use std::fs::File;
use std::path::{Path, PathBuf};

use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;

use tar::Archive;
use zip::ZipArchive;

/// Decompress a file into the output folder and return the root path extracted.
///
/// - Single files return the file path
/// - Archives return the root directory if possible, otherwise the common prefix
pub fn decompress(input: &Path, output: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(output)?;

    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

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

fn decompress_zip(input: &Path, output: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let mut archive = ZipArchive::new(file)?;

    let mut paths = Vec::new();

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
            paths.push(out_path);
        }
    }

    Ok(common_root(&paths, output))
}

// ---------------- TAR ----------------

fn unpack_tar(input: &Path, output: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let mut archive = Archive::new(file);

    let mut paths = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = output.join(entry.path()?);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&path)?;
        paths.push(path);
    }

    Ok(common_root(&paths, output))
}

// ---------------- GZIP ----------------

fn decompress_tar_gz(input: &Path, output: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let tar = GzDecoder::new(file);
    let mut archive = Archive::new(tar);

    let mut paths = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = output.join(entry.path()?);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&path)?;
        paths.push(path);
    }

    Ok(common_root(&paths, output))
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

fn decompress_tar_bz2(input: &Path, output: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let tar = BzDecoder::new(file);
    let mut archive = Archive::new(tar);

    let mut paths = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = output.join(entry.path()?);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&path)?;
        paths.push(path);
    }

    Ok(common_root(&paths, output))
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

// ---------------- HELPER ----------------

/// Determine the common root of extracted paths
fn common_root(paths: &[PathBuf], output: &Path) -> PathBuf {
    if paths.is_empty() {
        return output.to_path_buf();
    }

    let mut iter = paths.iter();
    let first = iter.next().unwrap();
    let mut components: Vec<_> = first.strip_prefix(output).unwrap().components().collect();

    for path in iter {
        let path_comps: Vec<_> = path.strip_prefix(output).unwrap().components().collect();
        components = components
            .iter()
            .zip(path_comps.iter())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| *a)
            .collect();
    }

    output.join(
        components
            .iter()
            .fold(PathBuf::new(), |acc, c| acc.join(c.as_os_str())),
    )
}

/*
/// Determine the common root of extracted paths
fn common_root(paths: &[PathBuf], output: &Path) -> PathBuf {
    if paths.is_empty() {
        return output.to_path_buf();
    }

    let mut iter = paths.iter();
    let first = iter.next().unwrap();
    let mut components: Vec<_> = first.strip_prefix(output).unwrap().components().collect();

    for path in iter {
        let path_comps: Vec<_> = path.strip_prefix(output).unwrap().components().collect();
        components = components
            .iter()
            .zip(path_comps.iter())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| *a)
            .collect();
    }

    output.join(components.iter().fold(PathBuf::new(), |acc, c| acc.join(c.as_os_str())))
}
*/
