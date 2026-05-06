use anyhow::{Result, anyhow};
use console::style;

use crate::services::packaging::RollbackManager;
use crate::services::storage::{
    metadata_storage::MetadataStorage, package_storage::PackageStorage,
    rollback_storage::RollbackStorage,
};
use crate::utils::static_paths::UpstreamPaths;

pub fn run(names: Vec<String>, prune: bool, dry_run: bool) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let mut metadata_storage = MetadataStorage::new(&paths.config.metadata_file)?;
    let rollback_file = RollbackManager::rollback_file_path(&paths);
    let mut rollback_storage = RollbackStorage::new(&rollback_file)?;

    let mut manager = RollbackManager::new(
        &paths,
        &mut package_storage,
        &mut metadata_storage,
        &mut rollback_storage,
    );

    if prune {
        return run_prune(names, dry_run, &mut manager);
    }

    if names.is_empty() {
        return Err(anyhow!(
            "At least one package name is required unless --prune is provided"
        ));
    }

    if dry_run {
        println!("{}", style("Dry run: rollback preview").bold());
        for name in &names {
            let Some(record) = manager.rollback_record(name) else {
                println!("{:<7} {:<28} no rollback data found", "[x]", name);
                continue;
            };

            let target = record
                .package_snapshot
                .install_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<missing>".to_string());
            println!(
                "{:<7} {:<28} would restore rollback from {} ({:?})",
                "[plan]", name, target, record.source
            );
        }
        println!("  actions: resolve only (no restore, no prune, no metadata changes)");
        return Ok(());
    }

    let mut restored = 0_u32;
    let mut failed = 0_u32;
    for name in &names {
        let mut msg = Some(|line: &str| println!("{line}"));
        match manager.restore_package(name, &mut msg) {
            Ok(_) => {
                println!("{:<7} {:<28} restored", "[✓]", name);
                restored += 1;
            }
            Err(err) => {
                println!("{:<7} {:<28} {}", "[!]", name, err);
                failed += 1;
            }
        }
    }

    if failed > 0 {
        println!(
            "{}",
            style(format!(
                "Rollback complete: {} restored, {} failed.",
                restored, failed
            ))
            .yellow()
        );
    } else {
        println!(
            "{}",
            style(format!(
                "Rollback complete: {} restored, 0 failed.",
                restored
            ))
            .green()
        );
    }

    Ok(())
}

fn run_prune(names: Vec<String>, dry_run: bool, manager: &mut RollbackManager<'_>) -> Result<()> {
    let target_names = if names.is_empty() {
        manager.rollback_packages()
    } else {
        names
    };

    if dry_run {
        println!("{}", style("Dry run: rollback prune preview").bold());
        if target_names.is_empty() {
            println!("No rollback artifacts to prune.");
            println!("  actions: resolve only (no prune, no metadata changes)");
            return Ok(());
        }

        for name in &target_names {
            if manager.rollback_record(name).is_some() {
                println!("{:<7} {:<28} would prune rollback artifact", "[plan]", name);
            } else {
                println!("{:<7} {:<28} no rollback data found", "[x]", name);
            }
        }
        println!("  actions: resolve only (no prune, no metadata changes)");
        return Ok(());
    }

    let mut pruned = 0_u32;
    let mut missing = 0_u32;
    for name in &target_names {
        if manager.prune_package(name)? {
            println!("{:<7} {:<28} pruned", "[✓]", name);
            pruned += 1;
        } else {
            println!("{:<7} {:<28} no rollback data found", "[x]", name);
            missing += 1;
        }
    }

    if target_names.is_empty() {
        println!("No rollback artifacts to prune.");
    } else {
        println!(
            "{}",
            style(format!(
                "Rollback prune complete: {} pruned, {} missing.",
                pruned, missing
            ))
            .green()
        );
    }

    Ok(())
}
