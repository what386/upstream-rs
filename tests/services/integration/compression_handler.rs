use super::decompress;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};
use tar::Builder;
use zip::write::SimpleFileOptions;

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("upstream-compress-test-{name}-{nanos}"))
}

fn cleanup(path: &PathBuf) -> io::Result<()> {
    fs::remove_dir_all(path)
}

fn create_gz_file(path: &PathBuf, content: &[u8]) {
    let file = File::create(path).expect("create .gz file");
    let mut encoder = GzEncoder::new(file, Compression::default());
    encoder
        .write_all(content)
        .expect("write compressed content");
    encoder.finish().expect("finish gzip");
}

fn create_tar_gz_with_file(path: &PathBuf, file_name: &str, content: &[u8]) {
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

fn create_zip_with_single_root_dir(path: &PathBuf) {
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
