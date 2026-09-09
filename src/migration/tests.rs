use super::*;
use crate::model::{
    Expectation, ExpectationStatus, ExpectationTarget, Language, Location, ReviewAnchor,
    ReviewCommentKind, ReviewStatus, ReviewThread, SymbolKind,
};

fn analysis(files: Vec<SourceFile>, symbols: Vec<Symbol>) -> ProjectAnalysis {
    ProjectAnalysis {
        schema_version: 1,
        project_name: "demo".to_owned(),
        root: ".".to_owned(),
        generated_unix_seconds: 0,
        source_revision: None,
        files,
        symbols,
        dependencies: Vec::new(),
        workflows: Vec::new(),
        workflow_priorities: Vec::new(),
        flows: Vec::new(),
        expectations: Vec::new(),
        verifications: Vec::new(),
        decisions: Vec::new(),
        works: Vec::new(),
        review_threads: Vec::new(),
        findings: Vec::new(),
    }
}

#[test]
fn detects_exact_file_and_symbol_migration_after_rename() {
    let old = analysis(
        vec![SourceFile {
            id: "f_old".to_owned(),
            path: "src/old.rs".to_owned(),
            language: Language::Rust,
            lines: 3,
            bytes: 20,
            content_hash: Some("same".to_owned()),
        }],
        vec![Symbol {
            id: "s_old".to_owned(),
            name: "run".to_owned(),
            kind: SymbolKind::Function,
            file_id: "f_old".to_owned(),
            content_hash: Some("region".to_owned()),
            location: Location {
                start_line: 1,
                start_column: 1,
                end_line: 3,
                end_column: 1,
            },
            entrypoint: false,
        }],
    );
    let new = analysis(
        vec![SourceFile {
            id: "f_new".to_owned(),
            path: "src/new.rs".to_owned(),
            language: Language::Rust,
            lines: 3,
            bytes: 20,
            content_hash: Some("same".to_owned()),
        }],
        vec![Symbol {
            id: "s_new".to_owned(),
            name: "run".to_owned(),
            kind: SymbolKind::Function,
            file_id: "f_new".to_owned(),
            content_hash: Some("region".to_owned()),
            location: Location {
                start_line: 1,
                start_column: 1,
                end_line: 3,
                end_column: 1,
            },
            entrypoint: false,
        }],
    );

    let migrations = source_migrations(&old, &new);
    assert_eq!(migrations.len(), 2);
    assert!(
        migrations
            .iter()
            .all(|migration| { migration.confidence == MigrationConfidence::Exact })
    );
}

#[test]
fn does_not_guess_ambiguous_symbol_migrations() {
    let file = |id: &str| SourceFile {
        id: id.to_owned(),
        path: format!("{id}.rs"),
        language: Language::Rust,
        lines: 1,
        bytes: 1,
        content_hash: Some(id.to_owned()),
    };
    let symbol = |id: &str, file_id: &str| Symbol {
        id: id.to_owned(),
        name: "run".to_owned(),
        kind: SymbolKind::Function,
        file_id: file_id.to_owned(),
        content_hash: None,
        location: Location {
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
        },
        entrypoint: false,
    };
    let old = analysis(vec![file("old")], vec![symbol("old_symbol", "old")]);
    let new = analysis(
        vec![file("new")],
        vec![symbol("new_a", "new"), symbol("new_b", "new")],
    );
    assert!(source_migrations(&old, &new).is_empty());
}

#[test]
fn flags_authored_records_that_still_use_old_source_ids() {
    let old = analysis(
        vec![SourceFile {
            id: "f_old".to_owned(),
            path: "src/old.rs".to_owned(),
            language: Language::Rust,
            lines: 1,
            bytes: 1,
            content_hash: Some("same".to_owned()),
        }],
        Vec::new(),
    );
    let mut new = analysis(
        vec![SourceFile {
            id: "f_new".to_owned(),
            path: "src/new.rs".to_owned(),
            language: Language::Rust,
            lines: 1,
            bytes: 1,
            content_hash: Some("same".to_owned()),
        }],
        Vec::new(),
    );
    new.expectations.push(Expectation {
        id: "e_old_target".to_owned(),
        target: ExpectationTarget::File,
        subject: Some("f_old".to_owned()),
        status: ExpectationStatus::Accepted,
        source: "human:test".to_owned(),
        title: "Keep the source behavior".to_owned(),
        detail: "The behavior remains intentional.".to_owned(),
    });

    let findings = source_migration_findings(&old, &new);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule_id, "SUS056");
    assert_eq!(findings[0].subject.as_deref(), Some("e_old_target"));
    assert!(findings[0].detail.contains("f_new"));
}

#[test]
fn accepted_migrations_retarget_subjects_and_source_anchors() {
    let old = analysis(
        vec![SourceFile {
            id: "f_old".to_owned(),
            path: "src/old.rs".to_owned(),
            language: Language::Rust,
            lines: 1,
            bytes: 1,
            content_hash: Some("same".to_owned()),
        }],
        Vec::new(),
    );
    let mut new = analysis(
        vec![SourceFile {
            id: "f_new".to_owned(),
            path: "src/new.rs".to_owned(),
            language: Language::Rust,
            lines: 1,
            bytes: 1,
            content_hash: Some("same".to_owned()),
        }],
        Vec::new(),
    );
    new.expectations.push(Expectation {
        id: "e1".to_owned(),
        target: ExpectationTarget::File,
        subject: Some("f_old".to_owned()),
        status: ExpectationStatus::Accepted,
        source: "human:test".to_owned(),
        title: "Keep it".to_owned(),
        detail: "The behavior remains intentional.".to_owned(),
    });
    new.review_threads.push(ReviewThread {
        id: "r1".to_owned(),
        target: ExpectationTarget::File,
        subject: None,
        anchor: Some(ReviewAnchor::Source {
            path: "src/old.rs".to_owned(),
            line: Some(1),
        }),
        parent: None,
        kind: ReviewCommentKind::Comment,
        status: ReviewStatus::Open,
        owner: None,
        source: "human:test".to_owned(),
        title: "Review".to_owned(),
        detail: "Please review this source.".to_owned(),
    });

    let migrations = source_migrations(&old, &new);
    let mapping = migrations
        .iter()
        .find(|migration| migration.kind == "file")
        .expect("file migration");
    let accepted = BTreeMap::from([(mapping.old_id.clone(), mapping.new_id.clone())]);
    let summary = apply_accepted_migrations(&mut new, &migrations, &accepted);

    assert_eq!(summary.expectations, 1);
    assert_eq!(summary.anchors, 1);
    assert_eq!(new.expectations[0].subject.as_deref(), Some("f_new"));
    assert_eq!(
        new.review_threads[0].anchor,
        Some(ReviewAnchor::Source {
            path: "src/new.rs".to_owned(),
            line: Some(1),
        })
    );
}
