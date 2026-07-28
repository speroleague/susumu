use anyhow::{Context, Result};
use susumu::model::{
    Confidence, ProjectAnalysis, ReviewStatus, Severity, VerificationStatus, WorkStatus,
};

use super::types::{
    CheckEvidenceJson, CheckItem, CheckItemJson, CheckJson, CheckProjectJson, CheckRecordsJson,
    CheckReport, CheckResultJson, CheckReviewJson, CheckSeverity, VerificationPostureJson,
    check_result_reason, check_severity_label, verification_evidence_posture,
};

pub(crate) fn check_report(analysis: &ProjectAnalysis, strict: bool) -> CheckReport {
    let mut items = check_items(analysis);
    items.sort_by(|left, right| {
        right
            .severity
            .cmp(&left.severity)
            .then_with(|| left.title.cmp(&right.title))
    });
    let critical = items
        .iter()
        .filter(|item| item.severity == CheckSeverity::Critical)
        .count();
    let warning = items
        .iter()
        .filter(|item| item.severity == CheckSeverity::Warning)
        .count();
    let attention = items
        .iter()
        .filter(|item| item.severity == CheckSeverity::Attention)
        .count();
    let failed = critical > 0 || (strict && warning > 0);
    CheckReport {
        items,
        critical,
        warning,
        attention,
        strict,
        failed,
    }
}

fn check_items(analysis: &ProjectAnalysis) -> Vec<CheckItem> {
    let mut items = Vec::new();
    add_finding_check_items(analysis, &mut items);
    add_verification_check_items(analysis, &mut items);
    add_work_check_items(analysis, &mut items);
    add_review_thread_check_items(analysis, &mut items);
    add_workflow_gap_check_items(analysis, &mut items);
    items
}

fn add_review_thread_check_items(analysis: &ProjectAnalysis, items: &mut Vec<CheckItem>) {
    for review in &analysis.review_threads {
        if review.status != ReviewStatus::Open {
            continue;
        }
        items.push(CheckItem {
            severity: CheckSeverity::Warning,
            title: format!("open review thread: {}", review.title),
            detail: format!(
                "{} owner={} target={} subject={} - reply, assign, or resolve this discussion.",
                review.detail,
                review.owner.as_deref().unwrap_or("-"),
                review.target,
                review.subject.as_deref().unwrap_or("-")
            ),
            source: review.source.clone(),
        });
    }
}

fn add_finding_check_items(analysis: &ProjectAnalysis, items: &mut Vec<CheckItem>) {
    for finding in &analysis.findings {
        let severity = match finding.severity {
            Severity::Error => CheckSeverity::Critical,
            Severity::Warning => CheckSeverity::Warning,
            Severity::Info if matches!(finding.rule_id.as_str(), "SUS023" | "SUS033") => {
                CheckSeverity::Warning
            }
            Severity::Info => continue,
        };
        items.push(CheckItem {
            severity,
            title: format!("{}: {}", finding.rule_id, finding.title),
            detail: finding.detail.clone(),
            source: finding.source.clone(),
        });
    }
}

fn add_verification_check_items(analysis: &ProjectAnalysis, items: &mut Vec<CheckItem>) {
    for verification in &analysis.verifications {
        let severity = match verification.status {
            VerificationStatus::Failed => CheckSeverity::Critical,
            VerificationStatus::Inconclusive => CheckSeverity::Warning,
            VerificationStatus::Passed => continue,
        };
        items.push(CheckItem {
            severity,
            title: format!(
                "{} verification: {}",
                verification.status,
                expectation_title(analysis, &verification.expectation_id)
            ),
            detail: format!(
                "{} method={} evidence={} basis={}",
                verification.detail,
                verification.method,
                verification.evidence.as_deref().unwrap_or("-"),
                verification.basis.as_deref().unwrap_or("-")
            ),
            source: verification.source.clone(),
        });
    }
}

fn add_work_check_items(analysis: &ProjectAnalysis, items: &mut Vec<CheckItem>) {
    for work in &analysis.works {
        if work.status != WorkStatus::Blocked {
            continue;
        }
        items.push(CheckItem {
            severity: CheckSeverity::Warning,
            title: format!("blocked work: {}", work.title),
            detail: format!(
                "{} evidence={} expectation={}",
                work.detail,
                work.evidence.as_deref().unwrap_or("-"),
                work.expectation_id.as_deref().unwrap_or("-")
            ),
            source: work.source.clone(),
        });
    }
}

fn add_workflow_gap_check_items(analysis: &ProjectAnalysis, items: &mut Vec<CheckItem>) {
    for workflow in &analysis.workflows {
        let Some(entry_symbol) = workflow.entry_symbol.as_deref() else {
            continue;
        };
        let gaps = analysis
            .flows
            .iter()
            .filter(|flow| {
                flow.from == entry_symbol
                    && flow.to.is_none()
                    && flow.confidence != Confidence::External
            })
            .count();
        if gaps == 0 {
            continue;
        }
        let label = if gaps == 1 { "edge" } else { "edges" };
        items.push(CheckItem {
            severity: CheckSeverity::Attention,
            title: format!("{} has unresolved call {label}", workflow.trigger),
            detail: format!(
                "{} has {gaps} unresolved outgoing call {label}. This may be framework, library, generated, or dynamic behavior.",
                workflow.trigger
            ),
            source: "susumu:derived".to_owned(),
        });
    }
}

pub(crate) fn print_check_report(
    analysis: &ProjectAnalysis,
    report: &CheckReport,
    max_items: usize,
) {
    println!("Susumu check: {}", analysis.project_name);
    println!("Root: {}", analysis.root);
    println!();
    println!("Evidence:");
    println!("  files: {}", analysis.files.len());
    println!("  workflows: {}", analysis.workflows.len());
    println!("  flows: {}", analysis.flows.len());
    println!("  findings: {}", analysis.findings.len());
    println!();
    println!("Records:");
    println!("  expectations: {}", analysis.expectations.len());
    println!("  verifications: {}", analysis.verifications.len());
    println!("  decisions: {}", analysis.decisions.len());
    println!("  work: {}", analysis.works.len());
    println!();
    println!("Review:");
    println!("  critical: {}", report.critical);
    println!("  warning: {}", report.warning);
    println!("  attention: {}", report.attention);
    println!();

    if report.items.is_empty() {
        println!("No review items.");
    } else {
        println!("Top review items:");
        for item in report.items.iter().take(max_items) {
            println!("  [{}] {}", check_severity_label(item.severity), item.title);
            println!("      source={}", item.source);
            println!("      {}", item.detail);
        }
        if report.items.len() > max_items {
            println!("  ... {} more", report.items.len() - max_items);
        }
    }

    println!();
    if report.failed {
        if report.critical > 0 {
            println!("Result: failed (critical review items present)");
        } else {
            println!("Result: failed (--strict treats warnings as blockers)");
        }
    } else if report.warning > 0 || report.attention > 0 {
        println!("Result: passed with review items");
    } else {
        println!("Result: passed");
    }
    if report.strict {
        println!("Mode: strict");
    }
}

pub(crate) fn print_check_json(analysis: &ProjectAnalysis, report: &CheckReport) -> Result<()> {
    let output = check_json(analysis, report);
    println!(
        "{}",
        serde_json::to_string_pretty(&output).context("could not serialize check report")?
    );
    Ok(())
}

pub(crate) fn check_json<'a>(
    analysis: &'a ProjectAnalysis,
    report: &'a CheckReport,
) -> CheckJson<'a> {
    CheckJson {
        project: CheckProjectJson {
            name: &analysis.project_name,
            root: &analysis.root,
            generated_unix_seconds: analysis.generated_unix_seconds,
        },
        evidence: CheckEvidenceJson {
            files: analysis.files.len(),
            workflows: analysis.workflows.len(),
            flows: analysis.flows.len(),
            findings: analysis.findings.len(),
        },
        records: CheckRecordsJson {
            expectations: analysis.expectations.len(),
            verifications: analysis.verifications.len(),
            decisions: analysis.decisions.len(),
            work: analysis.works.len(),
            review_threads: analysis.review_threads.len(),
        },
        review: CheckReviewJson {
            critical: report.critical,
            warning: report.warning,
            attention: report.attention,
        },
        result: CheckResultJson {
            status: if report.failed { "failed" } else { "passed" },
            failed: report.failed,
            strict: report.strict,
            reason: check_result_reason(report),
        },
        items: check_item_jsons(&report.items),
        verification_posture: analysis
            .verifications
            .iter()
            .map(|verification| VerificationPostureJson {
                id: verification.id.clone(),
                expectation_id: verification.expectation_id.clone(),
                posture: verification_evidence_posture(verification),
                trust_status: "not_authenticated",
            })
            .collect(),
    }
}

pub(crate) fn check_item_jsons(items: &[CheckItem]) -> Vec<CheckItemJson<'_>> {
    items
        .iter()
        .map(|item| CheckItemJson {
            severity: check_severity_label(item.severity),
            title: &item.title,
            detail: &item.detail,
            source: &item.source,
        })
        .collect()
}

fn expectation_title(analysis: &ProjectAnalysis, id: &str) -> String {
    analysis
        .expectations
        .iter()
        .find(|expectation| expectation.id == id)
        .map_or_else(|| id.to_owned(), |expectation| expectation.title.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use susumu::model::{
        Expectation, ExpectationStatus, ExpectationTarget, FlowEdge, Location, Verification,
        Workflow, WorkflowKind,
    };

    #[test]
    fn check_report_flags_failed_verifications() {
        let mut artifact = ProjectAnalysis {
            schema_version: susumu::model::SCHEMA_VERSION,
            project_name: "fixture".to_owned(),
            root: ".".to_owned(),
            generated_unix_seconds: 0,
            files: Vec::new(),
            symbols: Vec::new(),
            dependencies: Vec::new(),
            workflows: Vec::new(),
            workflow_priorities: Vec::new(),
            flows: Vec::new(),
            expectations: vec![Expectation {
                id: "e_checkout".to_owned(),
                target: ExpectationTarget::Project,
                subject: None,
                status: ExpectationStatus::Accepted,
                source: "human:test".to_owned(),
                title: "Checkout works".to_owned(),
                detail: "Checkout should work.".to_owned(),
            }],
            verifications: Vec::new(),
            decisions: Vec::new(),
            works: Vec::new(),
            review_threads: Vec::new(),
            findings: Vec::new(),
        };
        artifact.verifications.push(Verification {
            id: "v_checkout".to_owned(),
            expectation_id: "e_checkout".to_owned(),
            status: VerificationStatus::Failed,
            supersedes: None,
            execution: None,
            chain: None,
            method: "cargo test checkout".to_owned(),
            source: "ci:test".to_owned(),
            evidence: Some("run:checkout".to_owned()),
            basis: None,
            detail: "Checkout test failed.".to_owned(),
        });

        let report = check_report(&artifact, false);
        let json = check_json(&artifact, &report);

        assert!(report.failed);
        assert_eq!(report.critical, 1);
        assert_eq!(json.result.status, "failed");
        assert_eq!(json.items[0].title, "failed verification: Checkout works");
        assert_eq!(json.verification_posture[0].posture, "content_bound");
        assert_eq!(
            json.verification_posture[0].trust_status,
            "not_authenticated"
        );
    }

    #[test]
    fn external_workflow_calls_do_not_look_unresolved() {
        let mut artifact = ProjectAnalysis {
            schema_version: susumu::model::SCHEMA_VERSION,
            project_name: "fixture".to_owned(),
            root: ".".to_owned(),
            generated_unix_seconds: 0,
            files: Vec::new(),
            symbols: Vec::new(),
            dependencies: Vec::new(),
            workflows: vec![Workflow {
                id: "w_users".to_owned(),
                kind: WorkflowKind::Http,
                framework: "test".to_owned(),
                trigger: "GET /users".to_owned(),
                handler: Some("users".to_owned()),
                entry_symbol: Some("s_users".to_owned()),
                file_id: "file".to_owned(),
                confidence: Confidence::Exact,
                location: Location {
                    start_line: 1,
                    start_column: 1,
                    end_line: 1,
                    end_column: 1,
                },
            }],
            workflow_priorities: Vec::new(),
            flows: vec![FlowEdge {
                from: "s_users".to_owned(),
                to: None,
                call: "load_users".to_owned(),
                confidence: Confidence::External,
                location: Location {
                    start_line: 2,
                    start_column: 1,
                    end_line: 2,
                    end_column: 1,
                },
            }],
            expectations: Vec::new(),
            verifications: Vec::new(),
            decisions: Vec::new(),
            works: Vec::new(),
            review_threads: Vec::new(),
            findings: Vec::new(),
        };

        let report = check_report(&artifact, false);

        assert_eq!(report.attention, 0);
        assert!(report.items.is_empty());
        artifact.flows[0].confidence = Confidence::Ambiguous;
        let report = check_report(&artifact, false);
        assert_eq!(report.attention, 1);
    }
}
