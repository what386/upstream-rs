pub mod auth;
pub mod build;
pub mod cache;
pub mod changelog;
pub mod config;
pub mod docs;
pub mod doctor;
pub mod export;
pub mod find;
pub mod history;
pub mod hooks;
pub mod import;
pub mod info;
pub mod install;
pub mod list;
pub mod package;
pub mod probe;
pub mod reinstall;
pub mod remove;
pub mod rollback;
pub mod search;
pub mod upgrade;

use anyhow::{Result, bail};

use crate::{models::common::enums::Provider, output, storage::database::PackageDatabase};

pub fn resolve_new_package_name(
    override_name: Option<String>,
    source: &str,
    provider: Option<&Provider>,
    base_url: Option<&str>,
    database: &PackageDatabase,
) -> Result<String> {
    resolve_new_package_name_with(
        override_name,
        source,
        provider,
        base_url,
        |name| database.executable_alias_exists(name),
        |default| output::prompt_text("Package name", default),
    )
}

fn resolve_new_package_name_with<E, P>(
    override_name: Option<String>,
    source: &str,
    provider: Option<&Provider>,
    base_url: Option<&str>,
    mut package_exists: E,
    prompt: P,
) -> Result<String>
where
    E: FnMut(&str) -> Result<bool>,
    P: FnMut(Option<&str>) -> Result<String>,
{
    if let Some(name) = override_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        if package_exists(name)? {
            bail!("Package '{}' already exists", name);
        }

        return Ok(name.to_string());
    }

    let _ = (source, provider, base_url, package_exists, prompt);
    Ok(String::new())
}

#[cfg(test)]
mod package_name_tests {
    use std::cell::Cell;

    use super::resolve_new_package_name_with;
    use crate::models::common::enums::Provider;

    #[test]
    fn explicit_name_is_checked_without_prompting() {
        let prompted = Cell::new(false);
        let name = resolve_new_package_name_with(
            Some(" tool ".to_string()),
            "owner/repo",
            Some(&Provider::Github),
            None,
            |_| Ok(false),
            |_| {
                prompted.set(true);
                Ok("unused".to_string())
            },
        )
        .expect("resolve explicit name");

        assert_eq!(name, "tool");
        assert!(!prompted.get());
    }

    #[test]
    fn explicit_existing_name_is_rejected_without_prompting() {
        let prompted = Cell::new(false);
        let err = resolve_new_package_name_with(
            Some("tool".to_string()),
            "owner/repo",
            Some(&Provider::Github),
            None,
            |_| Ok(true),
            |_| {
                prompted.set(true);
                Ok("unused".to_string())
            },
        )
        .expect_err("reject duplicate");

        assert_eq!(err.to_string(), "Package 'tool' already exists");
        assert!(!prompted.get());
    }

    #[test]
    fn omitted_name_does_not_prompt_or_create_an_alias() {
        let prompted = Cell::new(false);
        let name = resolve_new_package_name_with(
            None,
            "owner/repo",
            Some(&Provider::Github),
            None,
            |_| Ok(false),
            |_| {
                prompted.set(true);
                Ok("unused".to_string())
            },
        )
        .expect("resolve inferred name");

        assert_eq!(name, "");
        assert!(!prompted.get());
    }

    #[test]
    fn omitted_name_does_not_check_or_prompt_for_alias_collisions() {
        let prompt_count = Cell::new(0);
        let name = resolve_new_package_name_with(
            None,
            "owner/tool",
            Some(&Provider::Github),
            None,
            |name| Ok(name == "tool"),
            |default| {
                let count = prompt_count.get();
                prompt_count.set(count + 1);
                match count {
                    0 => {
                        assert_eq!(default, Some("tool"));
                        Ok("tool".to_string())
                    }
                    1 => {
                        assert_eq!(default, None);
                        Ok("tool-alt".to_string())
                    }
                    _ => panic!("unexpected additional prompt"),
                }
            },
        )
        .expect("resolve replacement name");

        assert_eq!(name, "");
        assert_eq!(prompt_count.get(), 0);
    }

    #[test]
    fn direct_source_without_name_does_not_prompt() {
        let name = resolve_new_package_name_with(
            None,
            "https://example.invalid/tool.tar.gz",
            Some(&Provider::Direct),
            None,
            |_| Ok(false),
            |_| Ok("tool".to_string()),
        )
        .expect("resolve prompted name");

        assert_eq!(name, "");
    }
}
