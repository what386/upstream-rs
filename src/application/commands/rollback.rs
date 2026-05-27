use anyhow::{Result, anyhow};

use crate::application::output::{self, Status};
use crate::services::packaging::RollbackManager;
use crate::services::packaging::disk_impact::DiskImpact;
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
        println!("{}", output::title("Rollback preview"));
        output::print_local_disk_impact(&estimate_restore_impact(&names, &manager));
        for name in &names {
            let Some(record) = manager.rollback_record(name) else {
                output::status_line(Status::Fail, name, "no rollback data found");
                continue;
            };

            let target = record
                .package_snapshot
                .install_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<missing>".to_string());
            output::status_line(
                Status::Plan,
                name,
                format!("restore rollback from {} ({:?})", target, record.source),
            );
        }
        output::action_note("resolve only (no restore, no prune, no metadata changes)");
        return Ok(());
    }

    output::print_local_disk_impact(&estimate_restore_impact(&names, &manager));
    output::confirm_or_cancel(format!("Restore rollback for {} package(s)?", names.len()))?;

    let mut restored = 0_u32;
    let mut failed = 0_u32;
    for name in &names {
        let mut msg = Some(|line: &str| println!("{line}"));
        match manager.restore_package(name, &mut msg) {
            Ok(_) => {
                output::status_line(Status::Ok, name, "restored");
                restored += 1;
            }
            Err(err) => {
                output::status_line(Status::Fail, name, err);
                failed += 1;
            }
        }
    }

    if failed > 0 {
        println!(
            "{}",
            output::warning(format!(
                "Rollback complete: {} restored, {} failed.",
                restored, failed
            ))
        );
    } else {
        println!(
            "{}",
            output::success(format!(
                "Rollback complete: {} restored, 0 failed.",
                restored
            ))
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
        println!("{}", output::title("Rollback prune preview"));
        if target_names.is_empty() {
            println!("{}", output::warning("No rollback artifacts to prune."));
            output::action_note("resolve only (no prune, no metadata changes)");
            return Ok(());
        }

        output::print_local_disk_impact(&estimate_prune_impact(&target_names, manager));
        for name in &target_names {
            if manager.rollback_record(name).is_some() {
                output::status_line(Status::Plan, name, "prune rollback artifact");
            } else {
                output::status_line(Status::Fail, name, "no rollback data found");
            }
        }
        output::action_note("resolve only (no prune, no metadata changes)");
        return Ok(());
    }

    if !target_names.is_empty() {
        output::print_local_disk_impact(&estimate_prune_impact(&target_names, manager));
        output::confirm_or_cancel(format!(
            "Prune rollback artifacts for {} package(s)?",
            target_names.len()
        ))?;
    }

    let mut pruned = 0_u32;
    let mut missing = 0_u32;
    for name in &target_names {
        if manager.prune_package(name)? {
            output::status_line(Status::Ok, name, "pruned");
            pruned += 1;
        } else {
            output::status_line(Status::Fail, name, "no rollback data found");
            missing += 1;
        }
    }

    if target_names.is_empty() {
        println!("{}", output::warning("No rollback artifacts to prune."));
    } else {
        println!(
            "{}",
            output::success(format!(
                "Rollback prune complete: {} pruned, {} missing.",
                pruned, missing
            ))
        );
    }

    Ok(())
}

fn estimate_restore_impact(names: &[String], manager: &RollbackManager<'_>) -> DiskImpact {
    names
        .iter()
        .filter_map(|name| manager.estimate_restore_impact(name))
        .fold(DiskImpact::empty(), |total, impact| total + impact)
}

fn estimate_prune_impact(names: &[String], manager: &RollbackManager<'_>) -> DiskImpact {
    names
        .iter()
        .filter_map(|name| manager.estimate_prune_impact(name))
        .fold(DiskImpact::empty(), |total, impact| total + impact)
}
