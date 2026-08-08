pub mod add;
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

use crate::{
    models::common::enums::Provider, output, providers::discovery::infer_package_name,
    storage::database::PackageDatabase,
};

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
        |name| database.package_exists(name),
        |default| output::prompt_text("Package name", default),
    )
}

fn resolve_new_package_name_with<E, P>(
    override_name: Option<String>,
    source: &str,
    provider: Option<&Provider>,
    base_url: Option<&str>,
    mut package_exists: E,
    mut prompt: P,
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

    let mut default = infer_package_name(source, provider, base_url)?;
    loop {
        let name = prompt(default.as_deref())?;
        if !package_exists(&name)? {
            return Ok(name);
        }

        println!(
            "{}",
            output::warning(format!("Package '{}' already exists.", name))
        );
        default = None;
    }
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
    fn inferred_name_is_used_as_prompt_default() {
        let name = resolve_new_package_name_with(
            None,
            "owner/repo",
            Some(&Provider::Github),
            None,
            |_| Ok(false),
            |default| {
                assert_eq!(default, Some("repo"));
                Ok(default.expect("inferred default").to_string())
            },
        )
        .expect("resolve inferred name");

        assert_eq!(name, "repo");
    }

    #[test]
    fn duplicate_prompted_name_reprompts_without_the_conflicting_default() {
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

        assert_eq!(name, "tool-alt");
        assert_eq!(prompt_count.get(), 2);
    }

    #[test]
    fn source_without_inferred_name_prompts_without_a_default() {
        let name = resolve_new_package_name_with(
            None,
            "https://example.invalid/tool.tar.gz",
            Some(&Provider::Direct),
            None,
            |_| Ok(false),
            |default| {
                assert_eq!(default, None);
                Ok("tool".to_string())
            },
        )
        .expect("resolve prompted name");

        assert_eq!(name, "tool");
    }
}
