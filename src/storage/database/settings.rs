use crate::models::common::enums::TrustMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSettings {
    pub package_name: String,
    pub trust_mode: Option<TrustMode>,
}

impl PackageSettings {
    pub fn new(package_name: impl Into<String>) -> Self {
        Self {
            package_name: package_name.into(),
            trust_mode: None,
        }
    }
}
