use chrono::{DateTime, Utc};

use crate::models::common::enums::Filetype;
use crate::utils::filename_parser::{parse_arch, parse_filetype, parse_os};
use crate::utils::platform_info::{CpuArch, OSKind};

#[derive(Debug, Clone)]
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
            filetype,
            target_os: os,
            target_arch: arch,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Asset;
    use crate::models::common::enums::Filetype;
    use crate::utils::platform_info::{CpuArch, OSKind};
    use chrono::Utc;

    #[test]
    fn asset_new_derives_filetype_os_and_arch_from_name() {
        let asset = Asset::new(
            "https://example.invalid/tool-v1.2.3-linux-x86_64.tar.gz".to_string(),
            1,
            "tool-v1.2.3-linux-x86_64.tar.gz".to_string(),
            1024,
            Utc::now(),
        );

        assert_eq!(asset.filetype, Filetype::Archive);
        assert_eq!(asset.target_os, Some(OSKind::Linux));
        assert_eq!(asset.target_arch, Some(CpuArch::X86_64));
    }
}
