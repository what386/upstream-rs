use chrono::{DateTime, Utc};

use crate::models::common::enums::Filetype;
use crate::utils::platform_info::{CpuArch, OSKind}
use crate::models::common::cpu_arch::CpuArch;
use crate::utils::filename_parser::{parse_filetype, parse_arch, parse_os};

pub struct Asset {
    pub download_url: String,
    pub id: u64,
    pub name: String,
    pub size: u64,
    pub created_at: DateTime<Utc>,

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
        created_at: DateTime<Utc>,
    ) -> Self {
        let filetype = parse_filetype(&name);
        let os = parse_os(&name);
        let arch = parse_arch(&name);
        Self {
            download_url,
            id,
            name,
            size,
            created_at,
            filetype: filetype,
            target_os: os,
            target_arch: arch,
        }
    }
}
