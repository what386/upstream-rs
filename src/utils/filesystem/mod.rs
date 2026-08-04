use std::{fs, io, path::Path};

use anyhow::{Context, Result, anyhow};

pub mod atomic_ops;
pub mod manifest_sync;
pub mod safe_move;

/// Read UTF-8 or BOM-marked UTF-16 text without reading the file twice.
pub fn read_utf8_or_utf16(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("Failed to read '{}'", path.display()))?;
    decode_utf8_or_utf16(bytes)
        .with_context(|| format!("Failed to decode text file '{}'", path.display()))
}

fn decode_utf8_or_utf16(mut bytes: Vec<u8>) -> Result<String> {
    if let Some(contents) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(contents, u16::from_le_bytes, "UTF-16 LE");
    }
    if let Some(contents) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(contents, u16::from_be_bytes, "UTF-16 BE");
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        bytes.drain(..3);
    }
    String::from_utf8(bytes).map_err(|error| anyhow!(error))
}

fn decode_utf16(bytes: &[u8], decode_unit: fn([u8; 2]) -> u16, encoding: &str) -> Result<String> {
    if !bytes.len().is_multiple_of(2) {
        return Err(anyhow!("{encoding} text has an incomplete code unit"));
    }
    char::decode_utf16(
        bytes
            .chunks_exact(2)
            .map(|chunk| decode_unit([chunk[0], chunk[1]])),
    )
    .collect::<std::result::Result<String, _>>()
    .map_err(|error| anyhow!("Invalid {encoding} text: {error}"))
}

pub fn path_exists_no_follow(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => {
            Err(error).with_context(|| format!("Failed to inspect path '{}'", path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::decode_utf8_or_utf16;

    fn utf16_bytes(contents: &str, byte_order_mark: [u8; 2], big_endian: bool) -> Vec<u8> {
        let mut bytes = byte_order_mark.to_vec();
        for unit in contents.encode_utf16() {
            bytes.extend(if big_endian {
                unit.to_be_bytes()
            } else {
                unit.to_le_bytes()
            });
        }
        bytes
    }

    #[test]
    fn decode_text_supports_utf8_with_or_without_bom() {
        let contents = "abc123  tool.tar.gz\r\n";
        assert_eq!(
            decode_utf8_or_utf16(contents.as_bytes().to_vec()).expect("decode UTF-8"),
            contents
        );
        let bytes = [b"\xEF\xBB\xBF".as_slice(), contents.as_bytes()].concat();
        assert_eq!(
            decode_utf8_or_utf16(bytes).expect("decode UTF-8 BOM"),
            contents
        );
    }

    #[test]
    fn decode_text_supports_utf16_little_and_big_endian() {
        let contents = "4471b5a3  *powershell-linux-x64.tar.gz\r\n";
        let little_endian = utf16_bytes(contents, [0xFF, 0xFE], false);
        let big_endian = utf16_bytes(contents, [0xFE, 0xFF], true);
        assert_eq!(
            decode_utf8_or_utf16(little_endian).expect("decode UTF-16 LE"),
            contents
        );
        assert_eq!(
            decode_utf8_or_utf16(big_endian).expect("decode UTF-16 BE"),
            contents
        );
    }

    #[test]
    fn decode_text_rejects_invalid_encodings() {
        let incomplete_utf16 = [0xFF, 0xFE, b'a'];
        let invalid_utf8 = [0x80];
        assert!(
            decode_utf8_or_utf16(incomplete_utf16.to_vec())
                .expect_err("reject incomplete UTF-16")
                .to_string()
                .contains("incomplete code unit")
        );
        assert!(
            decode_utf8_or_utf16(invalid_utf8.to_vec())
                .expect_err("reject invalid UTF-8")
                .to_string()
                .contains("invalid utf-8")
        );
    }
}
