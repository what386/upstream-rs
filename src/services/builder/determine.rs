use std::path::Path;

use anyhow::{Result, anyhow};

use crate::services::builder::BuildProfile;
use crate::services::builder::profiles::BuildProfileHandler;

fn profile_name(profile: BuildProfile) -> &'static str {
    match profile {
        BuildProfile::Rust => "rust",
        BuildProfile::Dotnet => "dotnet",
        BuildProfile::Go => "go",
        BuildProfile::Zig => "zig",
        BuildProfile::Cmake => "cmake",
    }
}

pub fn determine_profile(
    workspace: &Path,
    requested: Option<BuildProfile>,
    handlers: &[Box<dyn BuildProfileHandler>],
) -> Result<BuildProfile> {
    if let Some(profile) = requested {
        return Ok(profile);
    }

    let detected: Vec<BuildProfile> = handlers
        .iter()
        .filter(|handler| handler.detect(workspace))
        .map(|handler| handler.profile())
        .collect();

    match detected.as_slice() {
        [single] => Ok(*single),
        [] => Err(anyhow!(
            "Could not auto-detect a build profile. Pass --build-profile (supported: rust, dotnet, go, zig, cmake)."
        )),
        many => {
            let names = many
                .iter()
                .map(|profile| profile_name(*profile))
                .collect::<Vec<_>>()
                .join(", ");
            Err(anyhow!(
                "Build profile detection is ambiguous ({names}). Re-run with --build-profile."
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::determine_profile;
    use crate::services::builder::BuildProfile;
    use crate::services::builder::profiles::BuildProfileHandler;

    struct FakeHandler {
        profile: BuildProfile,
        detect_value: bool,
    }

    impl BuildProfileHandler for FakeHandler {
        fn profile(&self) -> BuildProfile {
            self.profile
        }

        fn detect(&self, _workspace: &Path) -> bool {
            self.detect_value
        }

        fn run_build(
            &self,
            _workspace: &Path,
            _package_name: &str,
            _output_override: Option<&Path>,
            _line_callback: &mut Option<&mut dyn FnMut(&str)>,
        ) -> anyhow::Result<PathBuf> {
            unreachable!("run_build is not used in determine tests")
        }
    }

    #[test]
    fn requested_profile_bypasses_detection() {
        let handlers: Vec<Box<dyn BuildProfileHandler>> = vec![
            Box::new(FakeHandler {
                profile: BuildProfile::Rust,
                detect_value: false,
            }),
            Box::new(FakeHandler {
                profile: BuildProfile::Dotnet,
                detect_value: false,
            }),
            Box::new(FakeHandler {
                profile: BuildProfile::Go,
                detect_value: false,
            }),
            Box::new(FakeHandler {
                profile: BuildProfile::Zig,
                detect_value: false,
            }),
            Box::new(FakeHandler {
                profile: BuildProfile::Cmake,
                detect_value: false,
            }),
        ];
        let resolved = determine_profile(Path::new("."), Some(BuildProfile::Dotnet), &handlers)
            .expect("must resolve explicit profile");
        assert_eq!(resolved, BuildProfile::Dotnet);
    }

    #[test]
    fn returns_error_when_no_profiles_detected() {
        let handlers: Vec<Box<dyn BuildProfileHandler>> = vec![
            Box::new(FakeHandler {
                profile: BuildProfile::Rust,
                detect_value: false,
            }),
            Box::new(FakeHandler {
                profile: BuildProfile::Dotnet,
                detect_value: false,
            }),
            Box::new(FakeHandler {
                profile: BuildProfile::Go,
                detect_value: false,
            }),
            Box::new(FakeHandler {
                profile: BuildProfile::Zig,
                detect_value: false,
            }),
            Box::new(FakeHandler {
                profile: BuildProfile::Cmake,
                detect_value: false,
            }),
        ];
        let err = determine_profile(Path::new("."), None, &handlers).expect_err("must fail");
        assert!(err.to_string().contains("Could not auto-detect"));
    }

    #[test]
    fn returns_error_when_detection_is_ambiguous() {
        let handlers: Vec<Box<dyn BuildProfileHandler>> = vec![
            Box::new(FakeHandler {
                profile: BuildProfile::Rust,
                detect_value: true,
            }),
            Box::new(FakeHandler {
                profile: BuildProfile::Dotnet,
                detect_value: true,
            }),
            Box::new(FakeHandler {
                profile: BuildProfile::Go,
                detect_value: false,
            }),
            Box::new(FakeHandler {
                profile: BuildProfile::Zig,
                detect_value: false,
            }),
            Box::new(FakeHandler {
                profile: BuildProfile::Cmake,
                detect_value: false,
            }),
        ];
        let err = determine_profile(Path::new("."), None, &handlers).expect_err("must fail");
        assert!(err.to_string().contains("ambiguous"));
    }

    #[test]
    fn resolves_single_detected_profile() {
        let handlers: Vec<Box<dyn BuildProfileHandler>> = vec![
            Box::new(FakeHandler {
                profile: BuildProfile::Rust,
                detect_value: true,
            }),
            Box::new(FakeHandler {
                profile: BuildProfile::Dotnet,
                detect_value: false,
            }),
            Box::new(FakeHandler {
                profile: BuildProfile::Go,
                detect_value: false,
            }),
            Box::new(FakeHandler {
                profile: BuildProfile::Zig,
                detect_value: false,
            }),
            Box::new(FakeHandler {
                profile: BuildProfile::Cmake,
                detect_value: false,
            }),
        ];
        let resolved =
            determine_profile(Path::new("."), None, &handlers).expect("must resolve profile");
        assert_eq!(resolved, BuildProfile::Rust);
    }
}
