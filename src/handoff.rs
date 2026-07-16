use std::collections::BTreeSet;

use anyhow::{Context, Result};
use susumu::model::{ExpectationTarget, ProjectAnalysis};

use crate::{
    checks::check_item_jsons,
    review_types::{
        CheckEvidenceJson, CheckProjectJson, CheckRecordsJson, CheckReport, CheckResultJson,
        CheckReviewJson, HandoffJson, HandoffRecord, HandoffReport, HandoffWorkflow,
        check_result_reason, check_severity_label,
    },
};

pub(crate) fn handoff_report(analysis: &ProjectAnalysis, check: &CheckReport) -> HandoffReport {
    let top_workflows = handoff_top_workflows(analysis);
    let expectations_without_verification = handoff_expectations_without_verification(analysis);
    let work_needing_verification = handoff_work_needing_verification(analysis);
    let caveats = handoff_caveats(analysis, check);
    let next_actions = handoff_next_actions(
        check,
        &expectations_without_verification,
        &work_needing_verification,
        &caveats,
    );
    HandoffReport {
        top_workflows,
        expectations_without_verification,
        work_needing_verification,
        caveats,
        next_actions,
    }
}

fn handoff_top_workflows(analysis: &ProjectAnalysis) -> Vec<HandoffWorkflow> {
    let mut workflows = analysis
        .workflow_priorities
        .iter()
        .filter_map(|priority| {
            let workflow = analysis
                .workflows
                .iter()
                .find(|workflow| workflow.id == priority.workflow_id)?;
            let expectations = workflow_expectation_ids(analysis, &workflow.id);
            let verifications = analysis
                .verifications
                .iter()
                .filter(|verification| expectations.contains(&verification.expectation_id))
                .count();
            let work = analysis
                .works
                .iter()
                .filter(|work| {
                    (work.target == ExpectationTarget::Workflow
                        && work.subject.as_deref() == Some(workflow.id.as_str()))
                        || work
                            .expectation_id
                            .as_ref()
                            .is_some_and(|id| expectations.contains(id))
                })
                .count();
            Some(HandoffWorkflow {
                id: workflow.id.clone(),
                trigger: workflow.trigger.clone(),
                framework: workflow.framework.clone(),
                score: priority.score,
                expectations: expectations.len(),
                verifications,
                work,
                detail: priority.detail.clone(),
            })
        })
        .collect::<Vec<_>>();
    workflows.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.trigger.cmp(&right.trigger))
    });
    workflows
}

fn handoff_expectations_without_verification(analysis: &ProjectAnalysis) -> Vec<HandoffRecord> {
    let mut records = analysis
        .expectations
        .iter()
        .filter(|expectation| {
            !analysis
                .verifications
                .iter()
                .any(|verification| verification.expectation_id == expectation.id)
        })
        .map(|expectation| HandoffRecord {
            id: expectation.id.clone(),
            title: expectation.title.clone(),
            target: expectation.target.to_string(),
            subject: expectation.subject.clone(),
            source: expectation.source.clone(),
            reason: "no verification records linked".to_owned(),
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.id.cmp(&right.id));
    records
}

fn handoff_work_needing_verification(analysis: &ProjectAnalysis) -> Vec<HandoffRecord> {
    let mut records = analysis
        .works
        .iter()
        .filter(|work| {
            work.expectation_id.as_deref().is_some_and(|id| {
                !analysis
                    .verifications
                    .iter()
                    .any(|verification| verification.expectation_id == id)
            })
        })
        .map(|work| HandoffRecord {
            id: work.id.clone(),
            title: work.title.clone(),
            target: work.target.to_string(),
            subject: work.subject.clone(),
            source: work.source.clone(),
            reason: format!(
                "work addresses expectation `{}` but no verification is linked",
                work.expectation_id.as_deref().unwrap_or("-")
            ),
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.id.cmp(&right.id));
    records
}

fn handoff_caveats(analysis: &ProjectAnalysis, check: &CheckReport) -> Vec<String> {
    let mut caveats = Vec::new();
    let unresolved = analysis
        .flows
        .iter()
        .filter(|flow| flow.to.is_none())
        .count();
    if unresolved > 0 {
        caveats.push(format!(
            "{unresolved} call edge(s) are unresolved; dynamic/framework/library behavior may be hidden."
        ));
    }
    let stale = analysis
        .findings
        .iter()
        .filter(|finding| matches!(finding.rule_id.as_str(), "SUS023" | "SUS033"))
        .count();
    if stale > 0 {
        caveats.push(format!(
            "{stale} verification or decision record(s) are based on changed evidence."
        ));
    }
    if check.critical > 0 {
        caveats.push(format!(
            "{} critical review item(s) are present.",
            check.critical
        ));
    }
    if analysis.expectations.is_empty() {
        caveats.push(
            "No authored expectations are present; scanner evidence lacks business intent."
                .to_owned(),
        );
    }
    if analysis.verifications.is_empty() {
        caveats.push(
            "No verification records are present; do not treat work records as proof.".to_owned(),
        );
    }
    caveats
}

fn handoff_next_actions(
    check: &CheckReport,
    expectations_without_verification: &[HandoffRecord],
    work_needing_verification: &[HandoffRecord],
    caveats: &[String],
) -> Vec<String> {
    let mut actions = Vec::new();
    for item in check.items.iter().take(3) {
        actions.push(format!(
            "Review [{}] {}",
            check_severity_label(item.severity),
            item.title
        ));
    }
    for work in work_needing_verification.iter().take(3) {
        actions.push(format!(
            "Add or update verification for work `{}`.",
            work.id
        ));
    }
    for expectation in expectations_without_verification.iter().take(3) {
        actions.push(format!(
            "Add verification evidence for expectation `{}`.",
            expectation.id
        ));
    }
    if actions.is_empty() && caveats.is_empty() {
        actions.push("No immediate review actions were derived; inspect top workflows before making changes.".to_owned());
    } else if actions.is_empty() {
        actions.push("Review caveats before making changes.".to_owned());
    }
    dedup_strings(actions)
}

fn dedup_strings(mut values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
    values
}

fn workflow_expectation_ids(analysis: &ProjectAnalysis, workflow_id: &str) -> BTreeSet<String> {
    analysis
        .expectations
        .iter()
        .filter(|expectation| {
            expectation.target == ExpectationTarget::Workflow
                && expectation.subject.as_deref() == Some(workflow_id)
        })
        .map(|expectation| expectation.id.clone())
        .collect()
}

pub(crate) fn print_handoff_report(
    analysis: &ProjectAnalysis,
    check: &CheckReport,
    report: &HandoffReport,
    max_items: usize,
) {
    println!("Susumu handoff: {}", analysis.project_name);
    println!("Root: {}", analysis.root);
    println!();
    println!(
        "Evidence: {} files, {} workflows, {} flows, {} findings",
        analysis.files.len(),
        analysis.workflows.len(),
        analysis.flows.len(),
        analysis.findings.len()
    );
    println!(
        "Records: {} expectations, {} verifications, {} decisions, {} work",
        analysis.expectations.len(),
        analysis.verifications.len(),
        analysis.decisions.len(),
        analysis.works.len()
    );
    println!(
        "Review: {} critical, {} warning, {} attention",
        check.critical, check.warning, check.attention
    );
    println!();
    print_handoff_workflows(&report.top_workflows, max_items);
    print_handoff_review(check, max_items);
    print_handoff_records(
        "Expectations without verification",
        &report.expectations_without_verification,
        max_items,
    );
    print_handoff_records(
        "Work needing verification",
        &report.work_needing_verification,
        max_items,
    );
    print_string_section("Caveats", &report.caveats, max_items);
    print_string_section("Suggested next actions", &report.next_actions, max_items);
}

pub(crate) fn print_handoff_workflows(workflows: &[HandoffWorkflow], max_items: usize) {
    println!("Top workflows:");
    if workflows.is_empty() {
        println!("  none detected");
    }
    for workflow in workflows.iter().take(max_items) {
        println!(
            "  {:>3} {} ({}) expectations={} verifications={} work={}",
            workflow.score,
            workflow.trigger,
            workflow.framework,
            workflow.expectations,
            workflow.verifications,
            workflow.work
        );
        println!("      {}", workflow.detail);
    }
    if workflows.len() > max_items {
        println!("  ... {} more", workflows.len() - max_items);
    }
    println!();
}

fn print_handoff_review(check: &CheckReport, max_items: usize) {
    println!("Needs review:");
    if check.items.is_empty() {
        println!("  none derived");
    }
    for item in check.items.iter().take(max_items) {
        println!("  [{}] {}", check_severity_label(item.severity), item.title);
        println!("      {}", item.detail);
    }
    if check.items.len() > max_items {
        println!("  ... {} more", check.items.len() - max_items);
    }
    println!();
}

pub(crate) fn print_handoff_records(title: &str, records: &[HandoffRecord], max_items: usize) {
    println!("{title}:");
    if records.is_empty() {
        println!("  none");
    }
    for record in records.iter().take(max_items) {
        println!("  {}  {} ({})", record.id, record.title, record.reason);
    }
    if records.len() > max_items {
        println!("  ... {} more", records.len() - max_items);
    }
    println!();
}

pub(crate) fn print_string_section(title: &str, items: &[String], max_items: usize) {
    println!("{title}:");
    if items.is_empty() {
        println!("  none");
    }
    for item in items.iter().take(max_items) {
        println!("  - {item}");
    }
    if items.len() > max_items {
        println!("  ... {} more", items.len() - max_items);
    }
    println!();
}

pub(crate) fn print_handoff_json(
    analysis: &ProjectAnalysis,
    check: &CheckReport,
    report: &HandoffReport,
) -> Result<()> {
    let output = HandoffJson {
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
        },
        review: CheckReviewJson {
            critical: check.critical,
            warning: check.warning,
            attention: check.attention,
        },
        result: CheckResultJson {
            status: if check.failed { "failed" } else { "passed" },
            failed: check.failed,
            strict: check.strict,
            reason: check_result_reason(check),
        },
        top_workflows: &report.top_workflows,
        review_items: check_item_jsons(&check.items),
        expectations_without_verification: &report.expectations_without_verification,
        work_needing_verification: &report.work_needing_verification,
        caveats: &report.caveats,
        next_actions: &report.next_actions,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&output).context("could not serialize handoff report")?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review_types::{CheckItem, CheckSeverity};
    use susumu::model::{Expectation, ExpectationStatus, SCHEMA_VERSION};

    #[test]
    fn handoff_flags_expectations_without_verification() {
        let artifact = ProjectAnalysis {
            schema_version: SCHEMA_VERSION,
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
                id: "e_project".to_owned(),
                target: ExpectationTarget::Project,
                subject: None,
                status: ExpectationStatus::Accepted,
                source: "human:test".to_owned(),
                title: "Project expectation".to_owned(),
                detail: "The project expectation should be checked.".to_owned(),
            }],
            verifications: Vec::new(),
            decisions: Vec::new(),
            works: Vec::new(),
            findings: Vec::new(),
        };
        let check = CheckReport {
            items: vec![CheckItem {
                severity: CheckSeverity::Warning,
                title: "Review something".to_owned(),
                detail: "A review item should become a next action.".to_owned(),
                source: "test".to_owned(),
            }],
            critical: 0,
            warning: 1,
            attention: 0,
            strict: false,
            failed: false,
        };

        let handoff = handoff_report(&artifact, &check);

        assert_eq!(handoff.expectations_without_verification[0].id, "e_project");
        assert!(
            handoff
                .next_actions
                .iter()
                .any(|action| action.contains("Review [warning] Review something"))
        );
    }
}
