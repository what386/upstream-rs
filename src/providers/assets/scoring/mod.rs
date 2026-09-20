mod filetype;
mod name;
mod pattern;
mod platform;
mod size;

use crate::models::provider::Asset;
use crate::models::upstream::Package;
use crate::utils::platform::platform_info::ArchitectureInfo;

pub(super) use name::{is_auxiliary_asset_name, package_identity};
pub(super) use platform::{is_potentially_compatible, supported_filetypes_for_os};
pub(super) use size::size_profiles_by_filetype;

pub(super) fn score_asset(
    asset: &Asset,
    package: &Package,
    architecture: &ArchitectureInfo,
    size_profile: Option<&size::AssetSizeProfile>,
) -> i32 {
    let name = asset.name.to_lowercase();
    let package_name = package_identity(package);
    let mut score = 0;

    if package.filetype == crate::models::common::enums::Filetype::Auto {
        score += filetype::priority_score(asset.filetype);
    }

    score += name::primary_score(&name, &package_name);
    score += name::role_score(&name, &package_name);
    score += name::auxiliary_penalty(&name);
    score += platform::target_score(asset, architecture);
    score += filetype::format_score(asset.filetype, &name);
    score += size::absolute_score(asset.size);
    score += size::relative_score(asset.size, size_profile);
    score += pattern::score(&name, package);

    score
}
