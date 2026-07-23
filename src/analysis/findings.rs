use crate::model::{
    Decision, Expectation, ExpectationTarget, Finding, ProjectAnalysis, Severity, Work,
};

pub(crate) trait TargetedRecord {
    fn id(&self) -> &str;
    fn target(&self) -> ExpectationTarget;
}

impl TargetedRecord for Expectation {
    fn id(&self) -> &str {
        &self.id
    }

    fn target(&self) -> ExpectationTarget {
        self.target
    }
}

impl TargetedRecord for Decision {
    fn id(&self) -> &str {
        &self.id
    }

    fn target(&self) -> ExpectationTarget {
        self.target
    }
}

impl TargetedRecord for Work {
    fn id(&self) -> &str {
        &self.id
    }

    fn target(&self) -> ExpectationTarget {
        self.target
    }
}

pub(crate) fn project_subject_finding(
    rule_id: &str,
    noun: &str,
    record: &impl TargetedRecord,
) -> Finding {
    Finding {
        rule_id: rule_id.to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Info,
        title: format!("Project {noun} has a subject"),
        detail: format!(
            "{} is project-wide and has a subject id. Expected subject is `-`.",
            record.id()
        ),
        file_id: None,
        subject: Some(record.id().to_owned()),
        location: None,
    }
}

pub(crate) fn missing_subject_finding(
    rule_id: &str,
    noun: &str,
    record: &impl TargetedRecord,
) -> Finding {
    Finding {
        rule_id: rule_id.to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Warning,
        title: format!("{noun} is missing a target id"),
        detail: format!(
            "{} targets {} without a subject id.",
            record.id(),
            record.target()
        ),
        file_id: None,
        subject: Some(record.id().to_owned()),
        location: None,
    }
}

pub(crate) fn stale_subject_finding(
    rule_id: &str,
    noun: &str,
    record: &impl TargetedRecord,
    subject: &str,
) -> Finding {
    Finding {
        rule_id: rule_id.to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Warning,
        title: format!("{noun} target was not found"),
        detail: format!(
            "{} targets {} `{subject}`, but that id is not present in this artifact.",
            record.id(),
            record.target()
        ),
        file_id: None,
        subject: Some(record.id().to_owned()),
        location: None,
    }
}

pub(crate) fn expectation_subject_exists(
    analysis: &ProjectAnalysis,
    target: ExpectationTarget,
    subject: &str,
) -> bool {
    match target {
        ExpectationTarget::Project => true,
        ExpectationTarget::File => analysis.files.iter().any(|file| file.id == subject),
        ExpectationTarget::Symbol => analysis.symbols.iter().any(|symbol| symbol.id == subject),
        ExpectationTarget::Workflow => analysis
            .workflows
            .iter()
            .any(|workflow| workflow.id == subject),
    }
}
