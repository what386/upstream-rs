use anyhow::{Result, anyhow};
use console::style;

use crate::{
    application::operations::doctor_operation::{self, DoctorReport, Level},
    output::{self, Status},
};

fn status_for_level(level: Level) -> Status {
    match level {
        Level::Ok => Status::Ok,
        Level::Warn => Status::Warn,
        Level::Fail => Status::Fail,
    }
}

fn print_verbose_findings(report: &DoctorReport) {
    for finding in &report.findings {
        println!(
            "{} {}",
            output::status_label(status_for_level(finding.level)),
            finding.message
        );
    }
}

fn print_summary(report: &DoctorReport) {
    println!("{}/{} checks ok", report.ok, report.total_checks());
    if !report.warnings.is_empty() {
        println!();
        println!("{}", style("warnings:").yellow());
        for warning in &report.warnings {
            println!(" - {}", warning);
        }
    }

    if !report.failures.is_empty() {
        println!();
        println!("{}", style("failures:").red());
        for failure in &report.failures {
            println!(" - {}", failure);
        }
    }
}

fn print_hints(report: &DoctorReport) {
    if report.hints.is_empty() {
        return;
    }

    println!();
    println!("{}", style("Suggested fixes:").cyan());
    for hint in &report.hints {
        println!(" - {}", hint);
    }
}

pub fn run(names: Vec<String>, verbose: bool, fix: bool) -> Result<()> {
    println!("{}", style("Running upstream doctor...").cyan());

    let report = doctor_operation::run(names, fix)?;
    if verbose {
        print_verbose_findings(&report);
    }

    println!();
    print_summary(&report);
    print_hints(&report);

    if report.fail > 0 {
        return Err(anyhow!(
            "Doctor found {} failure(s). Resolve reported issues and retry.",
            report.fail
        ));
    }

    if report.warn > 0 {
        println!(
            "{}",
            style("Doctor completed with warnings. Review the items above.").yellow()
        );
    } else {
        println!("{}", style("Doctor completed successfully.").green());
    }

    Ok(())
}
