#[derive(Clone, Copy)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

pub struct DoctorFinding {
    pub level: Level,
    pub message: String,
}

pub struct DoctorReport {
    pub ok: u32,
    pub warn: u32,
    pub fail: u32,
    pub findings: Vec<DoctorFinding>,
    pub warnings: Vec<String>,
    pub failures: Vec<String>,
    pub hints: Vec<String>,
}

impl DoctorReport {
    pub(super) fn new() -> Self {
        Self {
            ok: 0,
            warn: 0,
            fail: 0,
            findings: Vec::new(),
            warnings: Vec::new(),
            failures: Vec::new(),
            hints: Vec::new(),
        }
    }

    pub(super) fn line(&mut self, level: Level, message: impl AsRef<str>) {
        let msg = message.as_ref();
        match level {
            Level::Ok => {
                self.ok += 1;
            }
            Level::Warn => {
                self.warn += 1;
                self.warnings.push(msg.to_string());
            }
            Level::Fail => {
                self.fail += 1;
                self.failures.push(msg.to_string());
            }
        }
        self.findings.push(DoctorFinding {
            level,
            message: msg.to_string(),
        });
    }

    pub fn total_checks(&self) -> u32 {
        self.ok + self.warn + self.fail
    }

    pub(super) fn hint(&mut self, hint: impl AsRef<str>) {
        let text = hint.as_ref().trim();
        if text.is_empty() {
            return;
        }
        if !self.hints.iter().any(|existing| existing == text) {
            self.hints.push(text.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DoctorReport, Level};

    #[test]
    fn doctor_report_hint_deduplicates_entries() {
        let mut report = DoctorReport::new();
        report.hint("Run upstream hooks init");
        report.hint("Run upstream hooks init");
        report.hint("Reinstall package");

        assert_eq!(report.hints.len(), 2);
        assert!(
            report
                .hints
                .contains(&"Run upstream hooks init".to_string())
        );
        assert!(report.hints.contains(&"Reinstall package".to_string()));
    }

    #[test]
    fn doctor_report_tracks_counts_and_findings() {
        let mut report = DoctorReport::new();
        report.line(Level::Ok, "ok");
        report.line(Level::Warn, "warn one");
        report.line(Level::Warn, "warn two");
        report.line(Level::Fail, "fail one");

        assert_eq!(report.ok, 1);
        assert_eq!(report.warn, 2);
        assert_eq!(report.fail, 1);
        assert_eq!(report.total_checks(), 4);
        assert_eq!(
            report.warnings,
            vec!["warn one".to_string(), "warn two".to_string()]
        );
        assert_eq!(report.failures, vec!["fail one".to_string()]);
        assert_eq!(report.findings.len(), 4);
        assert_eq!(report.findings[0].message, "ok");
        assert_eq!(report.findings[3].message, "fail one");
    }
}
