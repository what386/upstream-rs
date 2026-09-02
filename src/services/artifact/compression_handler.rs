use crate::application::cancellation;
use anyhow::{Result, anyhow};
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use std::borrow::Cow;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use tar::Archive;
use xz2::read::XzDecoder;
use zip::ZipArchive;
use zstd::Decoder as ZstdDecoder;

/// Decompress a file into the output folder and return the root path extracted.
///
/// - Single files return the file path
/// - Archives return a directory named after the archive file
/// - Single-directory archives are automatically flattened
pub fn decompress(input: &Path, output: &Path) -> Result<PathBuf> {
    decompress_with_progress(input, output, &mut |_, _| {})
}

/// Decompress an archive while reporting compressed input bytes consumed.
pub fn decompress_with_progress(
    input: &Path,
    output: &Path,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf> {
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

    let result = if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        decompress_tar_with(input, &extract_dir, progress, |r| Ok(GzDecoder::new(r)))
    } else if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") || name.ends_with(".tbz") {
        decompress_tar_with(input, &extract_dir, progress, |r| Ok(BzDecoder::new(r)))
    } else if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        decompress_tar_with(input, &extract_dir, progress, |r| Ok(XzDecoder::new(r)))
    } else if name.ends_with(".tar.zst") || name.ends_with(".tzst") {
        decompress_tar_with(input, &extract_dir, progress, |r| {
            ZstdDecoder::new(r).map_err(Into::into)
        })
    } else {
        match ext.as_str() {
            "zip" => decompress_zip(input, &extract_dir, progress),
            "gz" => decompress_single(input, &extract_dir, progress, |r| Ok(GzDecoder::new(r))),
            "bz2" => decompress_single(input, &extract_dir, progress, |r| Ok(BzDecoder::new(r))),
            "xz" => decompress_single(input, &extract_dir, progress, |r| Ok(XzDecoder::new(r))),
            "zst" => decompress_single(input, &extract_dir, progress, |r| {
                ZstdDecoder::new(r).map_err(Into::into)
            }),
            "tar" => unpack_tar(input, &extract_dir, progress),
            "7z" => decompress_7z(input, &extract_dir, progress),
            _ => Err(anyhow!("Unsupported format: {}", input.display())),
        }
    };

    finish_extraction(&extract_dir, result)
}

fn finish_extraction(path: &Path, result: Result<PathBuf>) -> Result<PathBuf> {
    if result.is_err() && cancellation::is_requested() {
        let _ = std::fs::remove_dir_all(path);
    }

    result
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

#[cfg(not(windows))]
fn resolve_relative_path_within_root(
    extract_dir: &Path,
    base_path: &Path,
    relative_path: &Path,
    absolute_err_label: &str,
    escape_err_label: &str,
) -> Result<PathBuf> {
    if relative_path.is_absolute() {
        return Err(anyhow!(
            "Archive {} has absolute path '{}'",
            absolute_err_label,
            relative_path.display()
        ));
    }

    let base_relative = base_path.strip_prefix(extract_dir).map_err(|_| {
        anyhow!(
            "Archive {} escapes extraction root: '{}'",
            escape_err_label,
            relative_path.display()
        )
    })?;

    let mut normalized_parts: Vec<PathBuf> = Vec::new();
    for component in base_relative.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized_parts.push(PathBuf::from(part)),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(anyhow!(
                    "Archive {} escapes extraction root: '{}'",
                    escape_err_label,
                    relative_path.display()
                ));
            }
        }
    }

    for component in relative_path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized_parts.push(PathBuf::from(part)),
            Component::ParentDir => {
                if normalized_parts.pop().is_none() {
                    return Err(anyhow!(
                        "Archive {} escapes extraction root: '{}'",
                        escape_err_label,
                        relative_path.display()
                    ));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(anyhow!(
                    "Archive {} has absolute path '{}'",
                    absolute_err_label,
                    relative_path.display()
                ));
            }
        }
    }

    Ok(normalized_parts
        .into_iter()
        .fold(extract_dir.to_path_buf(), |mut acc, part| {
            acc.push(part);
            acc
        }))
}

#[cfg(not(windows))]
fn safe_join_link_target(
    extract_dir: &Path,
    base_path: &Path,
    link_target: &Path,
) -> Result<PathBuf> {
    resolve_relative_path_within_root(
        extract_dir,
        base_path,
        link_target,
        "symlink target",
        "symlink target",
    )
}

fn unpack_tar_entries<R: Read>(archive: &mut Archive<R>, extract_dir: &Path) -> Result<PathBuf> {
    let mut paths = Vec::new();
    for entry in archive.entries()? {
        cancellation::check()?;
        let mut entry = entry?;
        let entry_path = entry.path()?;
        let path = safe_join_extract_path(extract_dir, &entry_path)?;
        let entry_type = entry.header().entry_type();

        if entry_type.is_hard_link() {
            let raw_target = entry.link_name()?.ok_or_else(|| {
                anyhow!("Archive hardlink '{}' has no target", entry_path.display())
            })?;

            let target_path = safe_join_extract_path(extract_dir, &raw_target)?;
            let metadata = std::fs::metadata(&target_path).map_err(|err| {
                anyhow!(
                    "Archive hardlink '{}' target is not available '{}': {}",
                    entry_path.display(),
                    raw_target.display(),
                    err
                )
            })?;

            if !metadata.is_file() {
                return Err(anyhow!(
                    "Archive hardlink '{}' target is not a regular file '{}'",
                    entry_path.display(),
                    raw_target.display()
                ));
            }

            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            std::fs::hard_link(&target_path, &path)?;
            paths.push(path);
            continue;
        }

        if entry_type.is_symlink() {
            #[cfg(windows)]
            {
                return Err(anyhow!(
                    "Archive contains unsupported symlink entry '{}'",
                    entry_path.display()
                ));
            }

            #[cfg(not(windows))]
            {
                let raw_target = entry.link_name()?.ok_or_else(|| {
                    anyhow!("Archive symlink '{}' has no target", entry_path.display())
                })?;

                let parent = path.parent().ok_or_else(|| {
                    anyhow!(
                        "Archive symlink entry has no parent directory '{}'",
                        entry_path.display()
                    )
                })?;

                let _target_path = safe_join_link_target(extract_dir, parent, &raw_target)?;
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                std::os::unix::fs::symlink(&raw_target, &path)?;
                paths.push(path);
                continue;
            }
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

fn decompress_zip(
    input: &Path,
    extract_dir: &Path,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf> {
    let file = progress_reader(input, progress)?;
    let mut archive = ZipArchive::new(file)?;
    let mut paths = Vec::new();
    for i in 0..archive.len() {
        cancellation::check()?;
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
            restore_zip_permissions(&file, &out_path)?;
            paths.push(out_path);
        }
    }

    common_root(&paths, extract_dir)
}

#[cfg(unix)]
fn restore_zip_permissions<R: Read + ?Sized>(
    file: &zip::read::ZipFile<'_, R>,
    path: &Path,
) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if let Some(mode) = file.unix_mode() {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o7777))?;
    }

    Ok(())
}

#[cfg(not(unix))]
fn restore_zip_permissions<R: Read + ?Sized>(
    _file: &zip::read::ZipFile<'_, R>,
    _path: &Path,
) -> Result<()> {
    Ok(())
}

// ---------------- 7Z ----------------

fn decompress_7z(
    input: &Path,
    extract_dir: &Path,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf> {
    let total = input.metadata()?.len();
    progress(0, total);
    let mut paths = Vec::new();
    sevenz_rust2::decompress_file_with_extract_fn(input, extract_dir, |entry, reader, _dest| {
        cancellation::check()
            .map_err(|err| sevenz_rust2::Error::Other(Cow::Owned(err.to_string())))?;
        let out_path = safe_join_extract_path(extract_dir, Path::new(entry.name()))
            .map_err(|err| sevenz_rust2::Error::Other(Cow::Owned(err.to_string())))?;

        let extracted = sevenz_rust2::default_entry_extract_fn(entry, reader, &out_path)?;
        if extracted && !entry.is_directory() {
            paths.push(out_path);
        }

        Ok(extracted)
    })?;
    progress(total, total);
    common_root(&paths, extract_dir)
}

// ---------------- TAR ----------------

fn unpack_tar(
    input: &Path,
    extract_dir: &Path,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf> {
    let file = progress_reader(input, progress)?;
    let mut archive = Archive::new(file);
    unpack_tar_entries(&mut archive, extract_dir)
}

/// Wraps a progress-tracked file reader in a decoder and unpacks it as a tar
/// archive. Shared by all `tar.*` variants (gz/bz2/xz/zst) — only the decoder
/// construction differs between them.
fn decompress_tar_with<'p, D: Read>(
    input: &Path,
    extract_dir: &Path,
    progress: &'p mut dyn FnMut(u64, u64),
    make_decoder: impl FnOnce(ProgressReader<'p, File>) -> Result<D>,
) -> Result<PathBuf> {
    let file = progress_reader(input, progress)?;
    let tar = make_decoder(file)?;
    let mut archive = Archive::new(tar);
    unpack_tar_entries(&mut archive, extract_dir)
}

/// Wraps a progress-tracked file reader in a decoder and streams it out to a
/// single output file named after the input's stem. Shared by all single-file
/// variants (gz/bz2/xz/zst) — only the decoder construction differs.
fn decompress_single<'p, D: Read>(
    input: &Path,
    extract_dir: &Path,
    progress: &'p mut dyn FnMut(u64, u64),
    make_decoder: impl FnOnce(ProgressReader<'p, File>) -> Result<D>,
) -> Result<PathBuf> {
    let file = progress_reader(input, progress)?;
    let mut decoder = make_decoder(file)?;
    let out_name = input
        .file_stem()
        .ok_or_else(|| anyhow!("Cannot derive output name"))?;

    let out_path = extract_dir.join(out_name);
    let mut out = File::create(&out_path)?;
    std::io::copy(&mut decoder, &mut out)?;
    Ok(out_path)
}

struct ProgressReader<'a, R> {
    reader: R,
    read: u64,
    total: u64,
    progress: &'a mut dyn FnMut(u64, u64),
}

impl<'a, R> ProgressReader<'a, R> {
    fn new(reader: R, total: u64, progress: &'a mut dyn FnMut(u64, u64)) -> Self {
        Self {
            reader,
            read: 0,
            total,
            progress,
        }
    }
}

fn progress_reader<'a>(
    input: &Path,
    progress: &'a mut dyn FnMut(u64, u64),
) -> Result<ProgressReader<'a, File>> {
    let total = input.metadata()?.len();
    Ok(ProgressReader::new(File::open(input)?, total, progress))
}

impl<R: Read> Read for ProgressReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if cancellation::is_requested() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Operation interrupted by CTRL-C",
            ));
        }

        let count = self.reader.read(buffer)?;
        self.read = self.read.saturating_add(count as u64);
        (self.progress)(self.read, self.total);
        Ok(count)
    }
}

impl<R: Seek> Seek for ProgressReader<'_, R> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let offset = self.reader.seek(position)?;
        self.read = offset;
        (self.progress)(self.read, self.total);
        Ok(offset)
    }
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
                cancellation::check()?;
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
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    #[cfg(not(windows))]
    use flate2::{Compression, write::GzEncoder};
    #[cfg(not(windows))]
    use tar::{Builder, Header};
    #[cfg(unix)]
    use zip::{ZipWriter, write::SimpleFileOptions};

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

    fn fixture_path(relative: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(relative)
    }

    fn assert_path_safety_error(err: &anyhow::Error) {
        let message = err.to_string();
        assert!(
            message.contains("absolute path") || message.contains("escapes extraction root"),
            "unexpected path safety error: {message}"
        );
    }

    #[test]
    fn decompress_single_gz_returns_decompressed_file() {
        let root = temp_root("single-gz");
        let input = fixture_path("compression/archives/hello.gz");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        let extracted = decompress(&input, &output).expect("decompress .gz");
        assert!(extracted.is_file());
        assert_eq!(fs::read(extracted).expect("read output"), b"hello-gz");
        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn decompress_single_zst_returns_decompressed_file() {
        let root = temp_root("single-zst");
        let input = fixture_path("compression/archives/hello.zst");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        let extracted = decompress(&input, &output).expect("decompress .zst");
        assert!(extracted.is_file());
        assert_eq!(fs::read(extracted).expect("read output"), b"hello-zst");
        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn decompress_tar_gz_extracts_archive_contents() {
        let root = temp_root("tar-gz");
        let input = fixture_path("compression/archives/tar/tar-gz-single-file.tar.gz");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
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
        let input = fixture_path("compression/archives/tar/tar-zst-single-file.tar.zst");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
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
        let input = fixture_path("compression/archives/zip/zip-single-root.zip");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
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

    #[cfg(unix)]
    #[test]
    fn decompress_zip_restores_unix_executable_permissions() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root("zip-permissions");
        let input = root.join("yazi-x86_64-unknown-linux-musl.zip");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");

        let file = fs::File::create(&input).expect("create archive");
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().unix_permissions(0o755);
        archive
            .start_file("yazi-x86_64-unknown-linux-musl/yazi", options)
            .expect("start binary");
        archive.write_all(b"binary").expect("write binary");
        archive.finish().expect("finish archive");

        let extracted_root = decompress(&input, &output).expect("decompress zip");
        let binary = extracted_root.join("yazi");
        assert_eq!(
            fs::metadata(binary)
                .expect("stat binary")
                .permissions()
                .mode()
                & 0o777,
            0o755
        );

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn decompress_7z_flattens_single_top_level_directory() {
        let root = temp_root("7z-flatten");
        let source = root.join("pkg");
        let archive = root.join("pkg.7z");
        let output = root.join("out");
        fs::create_dir_all(&source).expect("create source");
        fs::write(source.join("tool"), b"7z-content").expect("write source file");
        sevenz_rust2::compress_to_path(&source, &archive).expect("create .7z archive");
        let extracted_root = decompress(&archive, &output).expect("decompress .7z");
        let flattened_file = extracted_root.join("tool");
        assert!(flattened_file.exists());
        assert_eq!(
            fs::read(flattened_file).expect("read flattened file"),
            b"7z-content"
        );

        assert!(!extracted_root.join("pkg").exists());
        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn unsupported_format_returns_error() {
        let root = temp_root("unsupported");
        let input = fixture_path("compression/archives/input.unknown");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        let err = decompress(&input, &output).expect_err("must reject unsupported extension");
        assert!(err.to_string().contains("Unsupported format"));
        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn decompress_rejects_zip_path_traversal_entries() {
        let root = temp_root("zip-traversal");
        let input = fixture_path("compression/archives/zip/zip-path-traversal.zip");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        let err = decompress(&input, &output).expect_err("must reject traversal path");
        assert!(err.to_string().contains("escapes extraction root"));
        cleanup(&root).expect("cleanup");
    }

    #[cfg(not(windows))]
    #[test]
    fn decompress_rejects_zip_absolute_path_entries() {
        let root = temp_root("zip-absolute");
        let input = fixture_path("compression/archives/zip/zip-absolute-path.zip");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        let err = decompress(&input, &output).expect_err("must reject absolute path");
        assert!(err.to_string().contains("absolute path"));
        cleanup(&root).expect("cleanup");
    }

    #[cfg(windows)]
    #[test]
    fn decompress_rejects_zip_windows_absolute_path_entries() {
        let root = temp_root("zip-windows-absolute");
        let input = fixture_path("compression/archives/zip/zip-windows-absolute-path.zip");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        let err = decompress(&input, &output).expect_err("must reject windows absolute path");
        assert!(err.to_string().contains("absolute path"));
        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn decompress_allows_safe_tar_hardlink_entries() {
        let root = temp_root("tar-hardlink");
        let input = fixture_path("compression/archives/tar/tar-hardlink-safe.tar.gz");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        let extracted_root = decompress(&input, &output).expect("decompress with hardlink");
        let link_path = extracted_root.join("link.txt");
        let target_path = extracted_root.join("target.txt");
        assert!(link_path.exists());
        assert!(target_path.exists());
        assert_eq!(
            fs::read(link_path).expect("read through hardlink"),
            b"target-content"
        );

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn decompress_rejects_tar_hardlink_with_missing_target() {
        let root = temp_root("tar-hardlink-missing");
        let input = fixture_path("compression/archives/tar/tar-hardlink-missing-target.tar.gz");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        let err = decompress(&input, &output).expect_err("must reject missing hardlink target");
        assert!(err.to_string().contains("target is not available"));
        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn decompress_rejects_tar_hardlink_with_absolute_target() {
        let root = temp_root("tar-hardlink-abs");
        let input = fixture_path("compression/archives/tar/tar-hardlink-absolute-target.tar.gz");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        let err = decompress(&input, &output).expect_err("must reject absolute hardlink target");
        assert_path_safety_error(&err);
        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn decompress_rejects_tar_hardlink_with_traversal_target() {
        let root = temp_root("tar-hardlink-traversal");
        let input = fixture_path("compression/archives/tar/tar-hardlink-traversal-target.tar.gz");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        let err = decompress(&input, &output).expect_err("must reject traversal hardlink target");
        assert!(err.to_string().contains("escapes extraction root"));
        cleanup(&root).expect("cleanup");
    }

    #[cfg(windows)]
    #[test]
    fn decompress_rejects_tar_symlink_entries_on_windows() {
        let root = temp_root("tar-symlink-windows");
        let input = fixture_path("compression/archives/tar/tar-symlink-safe.tar.gz");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        let err = decompress(&input, &output).expect_err("must reject symlink entries on windows");
        assert!(err.to_string().contains("unsupported symlink entry"));
        cleanup(&root).expect("cleanup");
    }

    #[cfg(not(windows))]
    #[test]
    fn decompress_allows_safe_tar_symlink_entries() {
        let root = temp_root("tar-symlink");
        let input = fixture_path("compression/archives/tar/tar-symlink-safe.tar.gz");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        let extracted_root = decompress(&input, &output).expect("decompress with symlink");
        let link_path = extracted_root.join("link.txt");
        let target_path = extracted_root.join("target.txt");
        assert!(link_path.exists());
        assert!(target_path.exists());
        assert_eq!(
            fs::read(link_path).expect("read through symlink"),
            b"target-content"
        );

        cleanup(&root).expect("cleanup");
    }

    #[cfg(not(windows))]
    #[test]
    fn decompress_rejects_tar_symlink_with_absolute_target() {
        let root = temp_root("tar-symlink-abs");
        let input = fixture_path("compression/archives/tar/tar-symlink-absolute-target.tar.gz");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        let err = decompress(&input, &output).expect_err("must reject absolute symlink target");
        assert!(err.to_string().contains("absolute path"));
        cleanup(&root).expect("cleanup");
    }

    #[cfg(not(windows))]
    #[test]
    fn decompress_rejects_tar_symlink_with_traversal_target() {
        let root = temp_root("tar-symlink-traversal");
        let input = fixture_path("compression/archives/tar/tar-symlink-traversal-target.tar.gz");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        let err = decompress(&input, &output).expect_err("must reject traversal symlink target");
        assert!(err.to_string().contains("escapes extraction root"));
        cleanup(&root).expect("cleanup");
    }

    #[cfg(not(windows))]
    #[test]
    fn decompress_allows_tar_symlink_parent_relative_target_within_root() {
        let root = temp_root("tar-symlink-parent-rel-safe");
        let archive_path = root.join("nested-symlink.tar.gz");
        let output = root.join("out");
        fs::create_dir_all(&root).expect("create root");
        {
            let file = fs::File::create(&archive_path).expect("create archive file");
            let encoder = GzEncoder::new(file, Compression::default());
            let mut builder = Builder::new(encoder);

            let mut target_header = Header::new_gnu();
            let target_content = b"nested-target-content";
            target_header.set_size(target_content.len() as u64);
            target_header.set_mode(0o644);
            target_header.set_cksum();
            builder
                .append_data(&mut target_header, "dir/target.txt", &target_content[..])
                .expect("append target");

            let mut link_header = Header::new_gnu();
            link_header.set_entry_type(tar::EntryType::Symlink);
            link_header.set_size(0);
            link_header.set_mode(0o777);
            link_header
                .set_link_name("../target.txt")
                .expect("set symlink target");
            link_header.set_cksum();
            builder
                .append_data(&mut link_header, "dir/sub/link.txt", io::empty())
                .expect("append symlink");

            builder.finish().expect("finish archive");
        }

        let extracted_root = decompress(&archive_path, &output).expect("decompress with symlink");
        let link_path = extracted_root.join("sub/link.txt");
        assert!(link_path.exists());
        assert_eq!(
            fs::read(link_path).expect("read through symlink"),
            b"nested-target-content"
        );

        cleanup(&root).expect("cleanup");
    }
}
