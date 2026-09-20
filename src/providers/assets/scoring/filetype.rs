use std::path::Path;

use crate::models::common::enums::Filetype;

pub(super) fn priority_score(filetype: Filetype) -> i32 {
    #[cfg(target_os = "linux")]
    {
        match filetype {
            Filetype::AppImage => 160,
            Filetype::Archive => 60,
            Filetype::Compressed => 40,
            Filetype::Binary => 20,
            _ => -100,
        }
    }

    #[cfg(target_os = "windows")]
    {
        match filetype {
            Filetype::WinExe => 100,
            Filetype::Archive => 60,
            Filetype::Compressed => 40,
            _ => -100,
        }
    }

    #[cfg(target_os = "macos")]
    {
        match filetype {
            Filetype::Archive => 60,
            Filetype::Compressed => 40,
            Filetype::Binary => 20,
            _ => -100,
        }
    }
}

pub(super) fn format_score(filetype: Filetype, name: &str) -> i32 {
    let compression_score = match filetype {
        Filetype::Archive if name.ends_with(".tar.bz2") || name.ends_with(".tbz") => 15,
        Filetype::Archive if name.ends_with(".tar.gz") || name.ends_with(".tgz") => 10,
        Filetype::Archive if name.ends_with(".zip") => 5,
        Filetype::Compressed if name.ends_with(".bz2") => 10,
        Filetype::Compressed if name.ends_with(".gz") => 5,
        _ => 0,
    };

    let binary_score = if filetype == Filetype::Binary && Path::new(name).extension().is_none() {
        10
    } else {
        0
    };

    compression_score + binary_score
}
