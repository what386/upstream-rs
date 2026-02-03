use anyhow::Result;
use console::style;
use std::path::PathBuf;
use crate::{
    application::operations::export_operation::ExportOperation,
    services::storage::{package_storage::PackageStorage},
    utils::static_paths::UpstreamPaths,
};

pub async fn run_export(path: PathBuf, full: bool) -> Result<()> {
    let paths = UpstreamPaths::new();
    let package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let export_op = ExportOperation::new(&package_storage, &paths);

    if full {
        println!("{}", style("Creating full snapshot ...").cyan());
        export_op.export_snapshot(&path)?;
        println!(
            "{}",
            style(format!("Snapshot saved to '{}'", path.display())).green()
        );
    } else {
        println!("{}", style("Exporting package manifest ...").cyan());
        export_op.export_manifest(&path)?;
        println!(
            "{}",
            style(format!("Manifest saved to '{}'", path.display())).green()
        );
    }

    Ok(())
}
