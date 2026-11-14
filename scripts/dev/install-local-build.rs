//! Developer helper for sketchy-upgrade.sh; uses the normal managed replacement lifecycle.
use anyhow::{Context, Result, ensure};
use clap::Parser;
use std::{fs, path::PathBuf};
use upstream_rs::{
    models::common::{enums::Filetype, version::Version},
    services::{
        integration::CompletionManager,
        packaging::{InstallWorkspace, PackageActivator, PackageProgressEvent, PreparedInstall},
    },
    storage::{database::PackageDatabase, system::lock::LockStorage},
    utils::static_paths::UpstreamPaths,
};

#[derive(Parser)]
#[command(about = "Replace the managed upstream package with a local build")]
struct Args {
    binary: PathBuf,
    completions: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(args.binary.is_file(), "Local upstream binary is missing");
    ensure!(
        args.completions.is_dir(),
        "Local completions directory is missing"
    );

    let paths = UpstreamPaths::new()?;
    ensure!(
        paths.metadata.packages_database_file.is_file(),
        "No managed upstream installation found; install upstream before using this script"
    );

    let _lock = LockStorage::acquire(&paths, "install-local-build")?;
    let mut database = PackageDatabase::open(&paths.metadata.packages_database_file)?;
    let previous = database
        .get_package("github:what386/upstream-rs")?
        .context("Managed github:what386/upstream-rs package not found")?;

    ensure!(
        matches!(previous.filetype, Filetype::Binary | Filetype::WinExe),
        "Local updates require an upstream installation managed as a binary"
    );

    let installed = previous
        .install_path
        .as_ref()
        .context("Missing upstream install path")?;

    ensure!(installed.is_file(), "Installed upstream binary is missing");
    let relative = installed
        .strip_prefix(&paths.install.binaries_dir)
        .context("Installed upstream binary is outside the managed binaries directory")?;

    ensure!(
        !previous.executables.is_empty()
            && previous
                .executables
                .iter()
                .all(|exe| &exe.path == installed),
        "Expected upstream's executable aliases to refer to its managed binary"
    );

    let workspace = InstallWorkspace::new(&paths, &previous.id)?;
    let staged_binary = workspace.binaries_dir.join(relative);
    fs::create_dir_all(staged_binary.parent().context("Invalid binary path")?)?;
    fs::copy(&args.binary, &staged_binary).context("Failed to stage local upstream binary")?;
    if workspace.integration.shell_completions {
        CompletionManager::with_paths(workspace.completions()).install_from_root(
            previous.primary_executable_name(),
            &args.completions,
            &mut None::<fn(&str)>,
        )?;
    }

    let mut package = previous.clone();
    package.install_path = Some(staged_binary.clone());
    for executable in &mut package.executables {
        executable.path = staged_binary.clone();
    }

    package.version = Version::parse(env!("CARGO_PKG_VERSION"))?;
    // A local build isn't the previously downloaded release, even at the same version.
    package.release_tag = None;
    package.release_published_at = None;
    package.version_tag_template = None;
    package.last_upgraded = chrono::Utc::now();

    let activator = PackageActivator::new(&paths);
    let replacement = activator
        .replace(
            &previous,
            PreparedInstall::new(package, workspace),
            previous.icon_path.is_some(),
            &mut None::<fn(&str)>,
            &mut None::<fn(PackageProgressEvent)>,
        )
        .await?;

    let updated = activator.persist(&mut database, replacement)?;
    println!("Updated {} to local build {}", updated.id, updated.version);
    println!("Restart your shell to reload installed completions.");
    Ok(())
}
