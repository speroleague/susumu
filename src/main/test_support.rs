use super::*;
use susumu::model::{
    Confidence, Language, Location, SCHEMA_VERSION, SourceFile, Workflow, WorkflowKind,
};

pub(super) fn test_artifact() -> ProjectAnalysis {
    ProjectAnalysis {
        schema_version: SCHEMA_VERSION,
        project_name: "fixture".to_owned(),
        root: ".".to_owned(),
        generated_unix_seconds: 0,
        files: vec![
            SourceFile {
                id: "f_api".to_owned(),
                path: "src/api.ts".to_owned(),
                language: Language::TypeScript,
                lines: 10,
                bytes: 100,
                content_hash: Some("hash-api".to_owned()),
            },
            SourceFile {
                id: "f_routes".to_owned(),
                path: "src/routes.php".to_owned(),
                language: Language::Php,
                lines: 10,
                bytes: 100,
                content_hash: Some("hash-routes".to_owned()),
            },
        ],
        symbols: Vec::new(),
        dependencies: Vec::new(),
        workflows: vec![
            Workflow {
                id: "w_checkout".to_owned(),
                kind: WorkflowKind::Http,
                framework: "express-compatible".to_owned(),
                trigger: "POST /checkout".to_owned(),
                handler: Some("checkout".to_owned()),
                entry_symbol: Some("s_checkout".to_owned()),
                file_id: "f_api".to_owned(),
                confidence: Confidence::Exact,
                location: test_location(),
            },
            Workflow {
                id: "w_php_checkout".to_owned(),
                kind: WorkflowKind::Http,
                framework: "laravel".to_owned(),
                trigger: "POST /php-checkout".to_owned(),
                handler: Some("php_checkout".to_owned()),
                entry_symbol: Some("s_php_checkout".to_owned()),
                file_id: "f_routes".to_owned(),
                confidence: Confidence::Exact,
                location: test_location(),
            },
        ],
        workflow_priorities: Vec::new(),
        flows: Vec::new(),
        expectations: vec![Expectation {
            id: "e_checkout_sequence".to_owned(),
            target: ExpectationTarget::Workflow,
            subject: Some("w_checkout".to_owned()),
            status: ExpectationStatus::Accepted,
            source: "human:product".to_owned(),
            title: "Checkout reserves inventory before charging".to_owned(),
            detail: "Checkout must reserve inventory before payment capture.".to_owned(),
        }],
        verifications: Vec::new(),
        decisions: Vec::new(),
        works: Vec::new(),
        review_threads: Vec::new(),
        findings: Vec::new(),
    }
}

pub(super) const fn test_location() -> Location {
    Location {
        start_line: 1,
        start_column: 1,
        end_line: 1,
        end_column: 1,
    }
}

pub(super) fn test_commit(subject: &str, body: &str, changed_files: &[&str]) -> GitCommit {
    GitCommit {
        hash: "f240cd96a07f2ea7b14cc1932c58914ed0871575".to_owned(),
        author_name: "Codex".to_owned(),
        author_email: "codex@example.test".to_owned(),
        author_date: "2026-07-15T12:00:00-05:00".to_owned(),
        subject: subject.to_owned(),
        body: body.to_owned(),
        changed_files: changed_files
            .iter()
            .map(|path| (*path).to_owned())
            .collect(),
    }
}
