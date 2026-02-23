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
