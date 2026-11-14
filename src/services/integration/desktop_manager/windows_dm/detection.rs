use crate::{models::upstream::Package, services::integration::desktop_manager::archive_detection};

use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

pub(super) fn should_auto_integrate(package: &Package) -> bool {
    // A bundled GUI helper must not turn a console package into a desktop app.
    package
        .primary_executable()
        .is_some_and(|executable| windows_gui_executable(&executable.path))
        || archive_detection::package_has_desktop_data(package)
}

/// Reading a PE subsystem requires only the DOS header and the start of the PE
/// header, for both PE32 and PE32+. Console applications can have icons too.
fn windows_gui_executable(path: &Path) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut dos = [0; 64];
    if file.read_exact(&mut dos).is_err() || &dos[..2] != b"MZ" {
        return false;
    }
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
    let mut pe = [0; 94];
    if offset < 64
        || file.seek(SeekFrom::Start(offset)).is_err()
        || file.read_exact(&mut pe).is_err()
        || &pe[..4] != b"PE\0\0"
    {
        return false;
    }
    let characteristics = u16::from_le_bytes([pe[22], pe[23]]);
    let optional_size = u16::from_le_bytes([pe[20], pe[21]]);
    let magic = u16::from_le_bytes([pe[24], pe[25]]);
    characteristics & 0x0002 != 0 // executable image
        && characteristics & 0x2000 == 0 // not a DLL
        && optional_size >= 70
        && matches!(magic, 0x10b | 0x20b)
        && u16::from_le_bytes([pe[92], pe[93]]) == 2 // WINDOWS_GUI
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        common::enums::{Channel, Filetype, Provider},
        upstream::Package,
    };

    fn package(root: &Path) -> Package {
        let mut package = Package::with_defaults(
            "tool".into(),
            "owner/tool".into(),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        package.install_path = Some(root.to_path_buf());
        package
    }

    #[test]
    fn pe_detection_handles_gui_console_dll_and_malformed_headers() {
        let root = crate::utils::test_support::temp_root("desktop-detection", "pe");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("tool.exe");
        for magic in [0x10bu16, 0x20bu16] {
            let mut bytes = vec![0u8; 256];
            bytes[..2].copy_from_slice(b"MZ");
            bytes[60..64].copy_from_slice(&128u32.to_le_bytes());
            bytes[128..132].copy_from_slice(b"PE\0\0");
            bytes[148..150].copy_from_slice(&70u16.to_le_bytes());
            bytes[150..152].copy_from_slice(&2u16.to_le_bytes());
            bytes[152..154].copy_from_slice(&magic.to_le_bytes());
            bytes[220..222].copy_from_slice(&2u16.to_le_bytes());
            fs::write(&path, &bytes).unwrap();
            assert!(windows_gui_executable(&path));
            let mut package = package(&path);
            package.filetype = Filetype::Binary;
            package.executables = vec![crate::models::upstream::PackageExecutable {
                name: "tool".into(),
                path: path.clone(),
            }];
            assert!(should_auto_integrate(&package));
            bytes[220] = 3; // console subsystem, even if resources contain an icon
            fs::write(&path, &bytes).unwrap();
            assert!(!windows_gui_executable(&path));
            bytes[220] = 2;
            bytes[151] = 0x20; // DLL
            fs::write(&path, &bytes).unwrap();
            assert!(!windows_gui_executable(&path));
            fs::write(&path, &bytes[..160]).unwrap();
            assert!(!windows_gui_executable(&path));
            bytes[60..64].copy_from_slice(&u32::MAX.to_le_bytes());
            fs::write(&path, &bytes).unwrap();
            assert!(!windows_gui_executable(&path));
        }
        fs::remove_dir_all(root).unwrap();
    }
}
