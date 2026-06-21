use anyhow::{Result, anyhow};
use console::style;
use serde::Serialize;

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

pub async fn run(names: Vec<String>, verbose: bool, fix: bool, json: bool) -> Result<()> {
    if json {
        let report = doctor_operation::run(names, fix).await?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json_doctor_report(&report))?
        );
        if report.fail > 0 {
            return Err(anyhow!(
                "Doctor found {} failure(s). Resolve reported issues and retry.",
                report.fail
            ));
        }
        return Ok(());
    }

    println!("{}", style("Running upstream doctor...").cyan());

    let report = doctor_operation::run(names, fix).await?;
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

#[derive(Serialize)]
struct JsonDoctorReport {
    status: &'static str,
    ok: u32,
    warn: u32,
    fail: u32,
    total: u32,
    findings: Vec<JsonDoctorFinding>,
    warnings: Vec<String>,
    failures: Vec<String>,
    hints: Vec<String>,
}

#[derive(Serialize)]
struct JsonDoctorFinding {
    level: &'static str,
    message: String,
}

fn json_doctor_report(report: &DoctorReport) -> JsonDoctorReport {
    JsonDoctorReport {
        status: if report.fail > 0 {
            "failed"
        } else if report.warn > 0 {
            "warning"
        } else {
            "ok"
        },
        ok: report.ok,
        warn: report.warn,
        fail: report.fail,
        total: report.total_checks(),
        findings: report
            .findings
            .iter()
            .map(|finding| JsonDoctorFinding {
                level: level_label(finding.level),
                message: finding.message.clone(),
            })
            .collect(),
        warnings: report.warnings.clone(),
        failures: report.failures.clone(),
        hints: report.hints.clone(),
    }
}

fn level_label(level: Level) -> &'static str {
    match level {
        Level::Ok => "ok",
        Level::Warn => "warn",
        Level::Fail => "fail",
    }
}

#[cfg(test)]
mod tests {
    use super::json_doctor_report;
    use crate::application::operations::doctor_operation::{DoctorFinding, DoctorReport, Level};

    #[test]
    fn json_doctor_report_serializes_summary_and_findings() {
        let report = DoctorReport {
            ok: 2,
            warn: 1,
            fail: 1,
            findings: vec![
                DoctorFinding {
                    level: Level::Ok,
                    message: "config exists".to_string(),
                },
                DoctorFinding {
                    level: Level::Fail,
                    message: "symlink missing".to_string(),
                },
            ],
            warnings: vec!["PATH file missing".to_string()],
            failures: vec!["symlink missing".to_string()],
            hints: vec!["Run `upstream hooks init`.".to_string()],
        };

        let json =
            serde_json::to_value(json_doctor_report(&report)).expect("serialize doctor report");

        assert_eq!(json["status"], "failed");
        assert_eq!(json["total"], 4);
        assert_eq!(json["findings"][0]["level"], "ok");
        assert_eq!(json["findings"][1]["level"], "fail");
        assert_eq!(json["hints"][0], "Run `upstream hooks init`.");
    }
}
