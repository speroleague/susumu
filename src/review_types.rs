use serde::{Deserialize, Serialize};
use susumu::model::ProjectAnalysis;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CheckSeverity {
    Attention,
    Warning,
    Critical,
}

#[derive(Debug)]
pub(crate) struct CheckItem {
    pub(crate) severity: CheckSeverity,
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) source: String,
}

#[derive(Debug)]
pub(crate) struct CheckReport {
    pub(crate) items: Vec<CheckItem>,
    pub(crate) critical: usize,
    pub(crate) warning: usize,
    pub(crate) attention: usize,
    pub(crate) strict: bool,
    pub(crate) failed: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct CheckJson<'a> {
    pub(crate) project: CheckProjectJson<'a>,
    pub(crate) evidence: CheckEvidenceJson,
    pub(crate) records: CheckRecordsJson,
    pub(crate) review: CheckReviewJson,
    pub(crate) result: CheckResultJson<'a>,
    pub(crate) items: Vec<CheckItemJson<'a>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CheckProjectJson<'a> {
    pub(crate) name: &'a str,
    pub(crate) root: &'a str,
    pub(crate) generated_unix_seconds: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct CheckEvidenceJson {
    pub(crate) files: usize,
    pub(crate) workflows: usize,
    pub(crate) flows: usize,
    pub(crate) findings: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct CheckRecordsJson {
    pub(crate) expectations: usize,
    pub(crate) verifications: usize,
    pub(crate) decisions: usize,
    pub(crate) work: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct CheckReviewJson {
    pub(crate) critical: usize,
    pub(crate) warning: usize,
    pub(crate) attention: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct CheckResultJson<'a> {
    pub(crate) status: &'a str,
    pub(crate) failed: bool,
    pub(crate) strict: bool,
    pub(crate) reason: &'a str,
}

#[derive(Debug, Serialize)]
pub(crate) struct CheckItemJson<'a> {
    pub(crate) severity: &'a str,
    pub(crate) title: &'a str,
    pub(crate) detail: &'a str,
    pub(crate) source: &'a str,
}

#[derive(Debug)]
pub(crate) struct HandoffReport {
    pub(crate) top_workflows: Vec<HandoffWorkflow>,
    pub(crate) expectations_without_verification: Vec<HandoffRecord>,
    pub(crate) work_needing_verification: Vec<HandoffRecord>,
    pub(crate) caveats: Vec<String>,
    pub(crate) next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct HandoffWorkflow {
    pub(crate) id: String,
    pub(crate) trigger: String,
    pub(crate) framework: String,
    pub(crate) score: u32,
    pub(crate) expectations: usize,
    pub(crate) verifications: usize,
    pub(crate) work: usize,
    pub(crate) detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct HandoffRecord {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) target: String,
    pub(crate) subject: Option<String>,
    pub(crate) source: String,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ExpectationSupport {
    pub(crate) expectation_id: String,
    pub(crate) title: String,
    pub(crate) target: String,
    pub(crate) subject: Option<String>,
    pub(crate) target_observed: bool,
    pub(crate) verification: ExpectationVerificationSupport,
    pub(crate) work: usize,
    pub(crate) decisions: usize,
    pub(crate) findings: usize,
    pub(crate) support_status: String,
    pub(crate) reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ExpectationVerificationSupport {
    pub(crate) passed: usize,
    pub(crate) failed: usize,
    pub(crate) inconclusive: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ExpectationReadiness {
    pub(crate) expectation_id: String,
    pub(crate) title: String,
    pub(crate) target: String,
    pub(crate) subject: Option<String>,
    pub(crate) bucket: String,
    pub(crate) label: String,
    pub(crate) support_status: String,
    pub(crate) next_action: String,
}

pub(crate) const READINESS_BUCKETS: [(&str, &str); 5] = [
    ("failed_verification", "Failed verification"),
    ("missing_target", "Missing target"),
    ("needs_verification", "Has work, needs verification"),
    ("needs_work", "No linked work yet"),
    ("verified", "Verified"),
];

#[derive(Debug, Serialize)]
pub(crate) struct HandoffJson<'a> {
    pub(crate) project: CheckProjectJson<'a>,
    pub(crate) evidence: CheckEvidenceJson,
    pub(crate) records: CheckRecordsJson,
    pub(crate) review: CheckReviewJson,
    pub(crate) result: CheckResultJson<'a>,
    pub(crate) top_workflows: &'a [HandoffWorkflow],
    pub(crate) review_items: Vec<CheckItemJson<'a>>,
    pub(crate) expectations_without_verification: &'a [HandoffRecord],
    pub(crate) work_needing_verification: &'a [HandoffRecord],
    pub(crate) caveats: &'a [String],
    pub(crate) next_actions: &'a [String],
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewPacketJson<'a> {
    pub(crate) schema_version: &'static str,
    pub(crate) created_unix_seconds: u64,
    pub(crate) source: ReviewSourceJson,
    pub(crate) project: CheckProjectJson<'a>,
    pub(crate) evidence: CheckEvidenceJson,
    pub(crate) records: CheckRecordsJson,
    pub(crate) review: CheckReviewJson,
    pub(crate) result: CheckResultJson<'a>,
    pub(crate) top_workflows: &'a [HandoffWorkflow],
    pub(crate) review_items: Vec<CheckItemJson<'a>>,
    pub(crate) source_previews: Vec<ReviewSourcePreview>,
    pub(crate) expectation_support: Vec<ExpectationSupport>,
    pub(crate) expectation_readiness: Vec<ExpectationReadiness>,
    pub(crate) expectations_without_verification: &'a [HandoffRecord],
    pub(crate) work_needing_verification: &'a [HandoffRecord],
    pub(crate) caveats: &'a [String],
    pub(crate) next_actions: &'a [String],
    pub(crate) artifact: &'a ProjectAnalysis,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewSourceJson {
    pub(crate) input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ReviewPacketStored {
    pub(crate) schema_version: String,
    pub(crate) created_unix_seconds: u64,
    pub(crate) source: ReviewSourceStored,
    pub(crate) project: ReviewProjectStored,
    pub(crate) evidence: ReviewEvidenceStored,
    pub(crate) records: ReviewRecordsStored,
    pub(crate) review: ReviewCountsStored,
    pub(crate) result: ReviewResultStored,
    pub(crate) top_workflows: Vec<HandoffWorkflow>,
    pub(crate) review_items: Vec<ReviewItemStored>,
    #[serde(default)]
    pub(crate) source_previews: Vec<ReviewSourcePreview>,
    #[serde(default)]
    pub(crate) expectation_support: Vec<ExpectationSupport>,
    #[serde(default)]
    pub(crate) expectation_readiness: Vec<ExpectationReadiness>,
    pub(crate) expectations_without_verification: Vec<HandoffRecord>,
    pub(crate) work_needing_verification: Vec<HandoffRecord>,
    pub(crate) caveats: Vec<String>,
    pub(crate) next_actions: Vec<String>,
    pub(crate) artifact: ProjectAnalysis,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ReviewSourceStored {
    pub(crate) input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ReviewProjectStored {
    pub(crate) name: String,
    pub(crate) root: String,
    pub(crate) generated_unix_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ReviewEvidenceStored {
    pub(crate) files: usize,
    pub(crate) workflows: usize,
    pub(crate) flows: usize,
    pub(crate) findings: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ReviewRecordsStored {
    pub(crate) expectations: usize,
    pub(crate) verifications: usize,
    pub(crate) decisions: usize,
    pub(crate) work: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ReviewCountsStored {
    pub(crate) critical: usize,
    pub(crate) warning: usize,
    pub(crate) attention: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ReviewResultStored {
    pub(crate) status: String,
    pub(crate) failed: bool,
    pub(crate) strict: bool,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ReviewItemStored {
    pub(crate) severity: String,
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ReviewSourcePreview {
    pub(crate) file_id: String,
    pub(crate) path: String,
    pub(crate) language: String,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) highlight_start: usize,
    pub(crate) highlight_end: usize,
    pub(crate) lines: Vec<ReviewSourceLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ReviewSourceLine {
    pub(crate) number: usize,
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) html: String,
    #[serde(default)]
    pub(crate) tokens: Vec<ReviewSourceToken>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ReviewSourceToken {
    pub(crate) text: String,
    pub(crate) color: String,
}

pub(crate) const fn check_result_reason(report: &CheckReport) -> &'static str {
    if report.failed {
        if report.critical > 0 {
            "critical review items present"
        } else {
            "strict mode treats warnings as blockers"
        }
    } else if report.warning > 0 || report.attention > 0 {
        "passed with review items"
    } else {
        "passed"
    }
}

pub(crate) const fn check_severity_label(severity: CheckSeverity) -> &'static str {
    match severity {
        CheckSeverity::Critical => "critical",
        CheckSeverity::Warning => "warning",
        CheckSeverity::Attention => "attention",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_report_helpers_describe_review_state() {
        let report = CheckReport {
            items: Vec::new(),
            critical: 0,
            warning: 1,
            attention: 0,
            strict: false,
            failed: false,
        };

        assert_eq!(check_result_reason(&report), "passed with review items");
        assert_eq!(check_severity_label(CheckSeverity::Critical), "critical");
        assert_eq!(check_severity_label(CheckSeverity::Warning), "warning");
        assert_eq!(check_severity_label(CheckSeverity::Attention), "attention");
    }
}
