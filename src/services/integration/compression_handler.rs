use anyhow::{Result, anyhow};
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use tar::Archive;
use xz2::read::XzDecoder;
use zip::ZipArchive;
use zstd::Decoder;

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
    let file_name = input
        .file_name()
        .ok_or_else(|| anyhow!("Invalid archive path: no filename"))?
        .to_string_lossy();
    let name = file_name.to_lowercase();

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return decompress_tar_gz(input, &extract_dir);
    }
    if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") || name.ends_with(".tbz") {
        return decompress_tar_bz2(input, &extract_dir);
    }
    if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        return decompress_tar_xz(input, &extract_dir);
    }
    if name.ends_with(".tar.zst") || name.ends_with(".tzst") {
        return decompress_tar_zst(input, &extract_dir);
    }

    match ext.as_str() {
        "zip" => decompress_zip(input, &extract_dir),
        "gz" => decompress_gz_single(input, &extract_dir),
        "bz2" => decompress_bz2_single(input, &extract_dir),
        "xz" => decompress_xz_single(input, &extract_dir),
        "zst" => decompress_zst_single(input, &extract_dir),
        "tar" => unpack_tar(input, &extract_dir),
        _ => Err(anyhow!("Unsupported format: {}", input.display())),
    }
}

fn safe_join_extract_path(extract_dir: &Path, entry_path: &Path) -> Result<PathBuf> {
    if entry_path.is_absolute() {
        return Err(anyhow!(
            "Archive entry has absolute path '{}'",
            entry_path.display()
        ));
    }

    let mut out = PathBuf::from(extract_dir);
    for component in entry_path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => out.push(part),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(anyhow!(
                    "Archive entry escapes extraction root: '{}'",
                    entry_path.display()
                ));
            }
        }
    }

    Ok(out)
}

fn unpack_tar_entries<R: Read>(archive: &mut Archive<R>, extract_dir: &Path) -> Result<PathBuf> {
    let mut paths = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?;
        let path = safe_join_extract_path(extract_dir, &entry_path)?;

        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(anyhow!(
                "Archive contains unsupported link entry '{}'",
                entry_path.display()
            ));
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        entry.unpack(&path)?;
        paths.push(path);
    }

    common_root(&paths, extract_dir)
}

// ---------------- ZIP ----------------
fn decompress_zip(input: &Path, extract_dir: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let mut archive = ZipArchive::new(file)?;
    let mut paths = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let out_path = safe_join_extract_path(extract_dir, Path::new(file.name()))?;
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
    unpack_tar_entries(&mut archive, extract_dir)
}

// ---------------- ZST ----------------
fn decompress_tar_zst(input: &Path, extract_dir: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let tar = Decoder::new(file)?;
    let mut archive = Archive::new(tar);
    unpack_tar_entries(&mut archive, extract_dir)
}

fn decompress_zst_single(input: &Path, extract_dir: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let mut decoder = Decoder::new(file)?;
    let out_name = input
        .file_stem()
        .ok_or_else(|| anyhow!("Cannot derive output name"))?;
    let out_path = extract_dir.join(out_name);
    let mut out = File::create(&out_path)?;
    std::io::copy(&mut decoder, &mut out)?;
    Ok(out_path)
}


// ---------------- XZ ----------------
fn decompress_tar_xz(input: &Path, extract_dir: &Path) -> Result<PathBuf> {
    let file = File::open(input)?;
    let tar = XzDecoder::new(file);
    let mut archive = Archive::new(tar);
    unpack_tar_entries(&mut archive, extract_dir)
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
    unpack_tar_entries(&mut archive, extract_dir)
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
    unpack_tar_entries(&mut archive, extract_dir)
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
    let mut top_level_entries: std::collections::HashSet<PathBuf> =
        std::collections::HashSet::new();

    for path in paths {
        if let Ok(relative) = path.strip_prefix(extract_dir)
            && let Some(first_component) = relative.components().next()
        {
            top_level_entries.insert(extract_dir.join(first_component.as_os_str()));
        }
    }

    // If there's exactly one top-level entry and it's a directory, flatten it
    if top_level_entries.len() == 1 {
        let Some(single_dir) = top_level_entries.into_iter().next() else {
            return Ok(extract_dir.to_path_buf());
        };
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

#[cfg(test)]
mod tests {
    use super::decompress;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::fs::File;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};
    use tar::Builder;
    use zip::write::SimpleFileOptions;
    use zstd::stream::write::Encoder as ZstdEncoder;

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-compress-test-{name}-{nanos}"))
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn create_gz_file(path: &Path, content: &[u8]) {
        let file = File::create(path).expect("create .gz file");
        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder
            .write_all(content)
            .expect("write compressed content");
        encoder.finish().expect("finish gzip");
    }

    fn create_zst_file(path: &Path, content: &[u8]) {
        let file = File::create(path).expect("create .zst file");
        let mut encoder = ZstdEncoder::new(file, 0).expect("create zstd encoder");
        encoder
            .write_all(content)
            .expect("write compressed zstd content");
        encoder.finish().expect("finish zstd");
    }

    fn create_tar_gz_with_file(path: &Path, file_name: &str, content: &[u8]) {
        let file = File::create(path).expect("create .tar.gz");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, file_name, content)
            .expect("append tar entry");
        let encoder = builder.into_inner().expect("finalize tar");
        encoder.finish().expect("finalize gzip");
    }

    fn create_tar_zst_with_file(path: &Path, file_name: &str, content: &[u8]) {
        let file = File::create(path).expect("create .tar.zst");
        let encoder = ZstdEncoder::new(file, 0).expect("create zstd encoder");
        let mut builder = Builder::new(encoder);

        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, file_name, content)
            .expect("append tar entry");
        let encoder = builder.into_inner().expect("finalize tar");
        encoder.finish().expect("finalize zstd");
    }

    fn create_tar_gz_with_symlink(path: &Path, link_name: &str, target: &str) {
        let file = File::create(path).expect("create .tar.gz");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header
            .set_link_name(target)
            .expect("set tar symlink target");
        header.set_cksum();
        builder
            .append_data(&mut header, link_name, std::io::empty())
            .expect("append tar symlink entry");

        let encoder = builder.into_inner().expect("finalize tar");
        encoder.finish().expect("finalize gzip");
    }

    fn create_zip_with_single_root_dir(path: &Path) {
        let file = File::create(path).expect("create zip");
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        zip.add_directory("pkg/", options)
            .expect("add zip directory");
        zip.start_file("pkg/tool", options)
            .expect("start zip file entry");
        zip.write_all(b"zip-content").expect("write zip content");
        zip.finish().expect("finish zip");
    }

    fn create_zip_with_entry(path: &Path, entry_name: &str) {
        let file = File::create(path).expect("create zip");
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        zip.start_file(entry_name, options)
            .expect("start zip file entry");
        zip.write_all(b"zip-content").expect("write zip content");
        zip.finish().expect("finish zip");
    }

    #[test]
    fn decompress_single_gz_returns_decompressed_file() {
        let root = temp_root("single-gz");
        let input = root.join("hello.gz");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        create_gz_file(&input, b"hello-gz");

        let extracted = decompress(&input, &output).expect("decompress .gz");
        assert!(extracted.is_file());
        assert_eq!(fs::read(extracted).expect("read output"), b"hello-gz");

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn decompress_single_zst_returns_decompressed_file() {
        let root = temp_root("single-zst");
        let input = root.join("hello.zst");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        create_zst_file(&input, b"hello-zst");

        let extracted = decompress(&input, &output).expect("decompress .zst");
        assert!(extracted.is_file());
        assert_eq!(fs::read(extracted).expect("read output"), b"hello-zst");

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn decompress_tar_gz_extracts_archive_contents() {
        let root = temp_root("tar-gz");
        let input = root.join("bundle.tar.gz");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        create_tar_gz_with_file(&input, "tool.bin", b"tar-gz-content");

        let extracted_root = decompress(&input, &output).expect("decompress .tar.gz");
        let extracted_file = extracted_root.join("tool.bin");
        assert!(extracted_file.exists());
        assert_eq!(
            fs::read(extracted_file).expect("read extracted file"),
            b"tar-gz-content"
        );

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn decompress_tar_zst_extracts_archive_contents() {
        let root = temp_root("tar-zst");
        let input = root.join("bundle.tar.zst");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        create_tar_zst_with_file(&input, "tool.bin", b"tar-zst-content");

        let extracted_root = decompress(&input, &output).expect("decompress .tar.zst");
        let extracted_file = extracted_root.join("tool.bin");
        assert!(extracted_file.exists());
        assert_eq!(
            fs::read(extracted_file).expect("read extracted file"),
            b"tar-zst-content"
        );

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn decompress_zip_flattens_single_top_level_directory() {
        let root = temp_root("zip-flatten");
        let input = root.join("tool.zip");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        create_zip_with_single_root_dir(&input);

        let extracted_root = decompress(&input, &output).expect("decompress zip");
        let flattened_file = extracted_root.join("tool");
        assert!(flattened_file.exists());
        assert_eq!(
            fs::read(flattened_file).expect("read flattened file"),
            b"zip-content"
        );
        assert!(!extracted_root.join("pkg").exists());

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn unsupported_format_returns_error() {
        let root = temp_root("unsupported");
        let input = root.join("input.unknown");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        fs::write(&input, b"data").expect("write input");

        let err = decompress(&input, &output).expect_err("must reject unsupported extension");
        assert!(err.to_string().contains("Unsupported format"));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn decompress_rejects_zip_path_traversal_entries() {
        let root = temp_root("zip-traversal");
        let input = root.join("tool.zip");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        create_zip_with_entry(&input, "../escape.txt");

        let err = decompress(&input, &output).expect_err("must reject traversal path");
        assert!(err.to_string().contains("escapes extraction root"));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn decompress_rejects_tar_symlink_entries() {
        let root = temp_root("tar-symlink");
        let input = root.join("bundle.tar.gz");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        create_tar_gz_with_symlink(&input, "pkg-link", "/tmp/outside");

        let err = decompress(&input, &output).expect_err("must reject symlink entry");
        assert!(err.to_string().contains("unsupported link entry"));

        cleanup(&root).expect("cleanup");
    }
}
