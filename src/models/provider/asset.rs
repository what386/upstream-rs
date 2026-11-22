use jiff::Timestamp;

use crate::models::common::enums::Filetype;
use crate::providers::assets::filenames::parser::{parse_arch, parse_filetype, parse_os};
use crate::utils::platform::platform_info::{CpuArch, OSKind};

#[derive(Debug, Clone)]
pub struct Asset {
    /// Download location shown to users and used to resolve relative zsync URLs.
    pub download_url: String,
    /// Provider API endpoint used to fetch authenticated asset content.
    pub api_download_url: Option<String>,
    pub id: u64,
    pub name: String,
    pub size: u64,
    pub expected_sha256: Option<String>,
    pub created_at: Timestamp,

    // computed from name
    pub filetype: Filetype,
    pub target_os: Option<OSKind>,
    pub target_arch: Option<CpuArch>,
}

impl Asset {
    pub fn new(
        download_url: String,
        id: u64,
        name: String,
        size: u64,
        created_at: Timestamp,
    ) -> Self {
        let filetype = parse_filetype(&name);
        Self::with_filetype(download_url, id, name, size, created_at, filetype)
    }

    pub fn with_filetype(
        download_url: String,
        id: u64,
        name: String,
        size: u64,
        created_at: Timestamp,
        filetype: Filetype,
    ) -> Self {
        let os = parse_os(&name);
        let arch = parse_arch(&name);
        Self {
            download_url,
            api_download_url: None,
            id,
            name,
            size,
            expected_sha256: None,
            created_at,
            filetype,
            target_os: os,
            target_arch: arch,
        }
    }
}
