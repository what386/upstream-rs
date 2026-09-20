use std::collections::HashMap;

use crate::models::common::enums::Filetype;
use crate::models::provider::Asset;
use crate::utils::math::median_sorted;

use super::platform::supported_filetypes_for_os;

#[derive(Debug, Clone, Copy)]
pub(in crate::providers::assets) struct AssetSizeProfile {
    median: u64,
}

impl AssetSizeProfile {
    const OUTLIER_FACTOR: u64 = 10;
    const EXPECTED_RANGE_FACTOR: u64 = 2;

    fn from_assets<'a>(assets: impl IntoIterator<Item = &'a Asset>) -> Option<Self> {
        let mut sizes: Vec<u64> = assets
            .into_iter()
            .map(|asset| asset.size)
            .filter(|size| *size > 0)
            .collect();

        if sizes.is_empty() {
            return None;
        }

        sizes.sort_unstable();
        let raw_median = median_sorted(&sizes)?;
        let lower_bound = raw_median.div_ceil(Self::OUTLIER_FACTOR);
        let upper_bound = raw_median.saturating_mul(Self::OUTLIER_FACTOR);
        let trimmed: Vec<u64> = sizes
            .iter()
            .copied()
            .filter(|size| *size >= lower_bound && *size <= upper_bound)
            .collect();

        Some(Self {
            median: median_sorted(if trimmed.is_empty() { &sizes } else { &trimmed })?,
        })
    }

    fn is_outside_expected_range(&self, size: u64) -> bool {
        size > 0
            && self.median > 0
            && (size < self.median.div_ceil(Self::EXPECTED_RANGE_FACTOR)
                || size > self.median.saturating_mul(Self::EXPECTED_RANGE_FACTOR))
    }
}

pub(in crate::providers::assets) fn size_profiles_by_filetype(
    assets: &[&Asset],
) -> HashMap<Filetype, AssetSizeProfile> {
    let mut profiles = HashMap::new();

    for filetype in supported_filetypes_for_os() {
        if let Some(profile) = AssetSizeProfile::from_assets(
            assets
                .iter()
                .copied()
                .filter(|asset| asset.filetype == filetype),
        ) {
            profiles.insert(filetype, profile);
        }
    }

    profiles
}

pub(super) fn absolute_score(size: u64) -> i32 {
    match size {
        0..=3_999 => -80,
        4_000..=15_999 => -60,
        16_000..=49_999 => -40,
        50_000..=99_999 => -20,
        500_000_001.. => -10,
        _ => 0,
    }
}

pub(super) fn relative_score(size: u64, profile: Option<&AssetSizeProfile>) -> i32 {
    if profile.is_some_and(|profile| profile.is_outside_expected_range(size)) {
        -20
    } else {
        0
    }
}
