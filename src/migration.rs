use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::model::{
    Finding, ProjectAnalysis, ReviewAnchor, Severity, SourceFile, Symbol, Workflow,
};

/// A source identity that can be carried from one revision to another.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceMigration {
    pub kind: String,
    pub old_id: String,
    pub new_id: String,
    pub old_path: String,
    pub new_path: String,
    pub confidence: MigrationConfidence,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MigrationConfidence {
    Exact,
    Candidate,
}

/// Finds source identities that likely survived a rename or refactor.
///
/// This produces explicit migration candidates; it never rewrites authored records or claims
/// that a refactored symbol is the same without a person or a later migration command accepting it.
#[must_use]
pub fn source_migrations(old: &ProjectAnalysis, new: &ProjectAnalysis) -> Vec<SourceMigration> {
    let file_matches = match_files(&old.files, &new.files);
    let mut migrations = file_matches
        .iter()
        .filter(|(old_file, new_file, _)| old_file.id != new_file.id)
        .map(|(old_file, new_file, confidence)| SourceMigration {
            kind: "file".to_owned(),
            old_id: old_file.id.clone(),
            new_id: new_file.id.clone(),
            old_path: old_file.path.clone(),
            new_path: new_file.path.clone(),
            confidence: *confidence,
            detail: format!("{} moved to {}", old_file.path, new_file.path),
        })
        .collect::<Vec<_>>();

    for (old_file, new_file, file_confidence) in file_matches {
        let old_symbols = old
            .symbols
            .iter()
            .filter(|symbol| symbol.file_id == old_file.id);
        let new_symbols = new
            .symbols
            .iter()
            .filter(|symbol| symbol.file_id == new_file.id)
            .collect::<Vec<_>>();
        for old_symbol in old_symbols {
            let candidates = new_symbols
                .iter()
                .filter(|symbol| same_symbol_shape(old_symbol, symbol))
                .copied()
                .collect::<Vec<_>>();
            if candidates.len() != 1 {
                continue;
            }
            let new_symbol = candidates[0];
            if old_symbol.id == new_symbol.id {
                continue;
            }
            let exact = old_symbol.content_hash.is_some()
                && old_symbol.content_hash == new_symbol.content_hash;
            migrations.push(SourceMigration {
                kind: "symbol".to_owned(),
                old_id: old_symbol.id.clone(),
                new_id: new_symbol.id.clone(),
                old_path: old_file.path.clone(),
                new_path: new_file.path.clone(),
                confidence: if exact {
                    MigrationConfidence::Exact
                } else {
                    MigrationConfidence::Candidate
                },
                detail: if exact {
                    format!("{} preserved its parsed source region", old_symbol.name)
                } else {
                    format!(
                        "{} retained its name and kind after a source change",
                        old_symbol.name
                    )
                },
            });
        }

        let old_workflows = old
            .workflows
            .iter()
            .filter(|workflow| workflow.file_id == old_file.id);
        let new_workflows = new
            .workflows
            .iter()
            .filter(|workflow| workflow.file_id == new_file.id)
            .collect::<Vec<_>>();
        for old_workflow in old_workflows {
            let candidates = new_workflows
                .iter()
                .filter(|workflow| same_workflow_shape(old_workflow, workflow))
                .copied()
                .collect::<Vec<_>>();
            if candidates.len() != 1 {
                continue;
            }
            let new_workflow = candidates[0];
            if old_workflow.id == new_workflow.id {
                continue;
            }
            migrations.push(SourceMigration {
                kind: "workflow".to_owned(),
                old_id: old_workflow.id.clone(),
                new_id: new_workflow.id.clone(),
                old_path: old_file.path.clone(),
                new_path: new_file.path.clone(),
                confidence: file_confidence,
                detail: format!(
                    "{} workflow moved with its source file",
                    old_workflow.trigger
                ),
            });
        }
    }

    migrations.sort_by(|left, right| {
        (&left.kind, &left.old_id, &left.new_id).cmp(&(&right.kind, &right.old_id, &right.new_id))
    });
    migrations
}

/// Reports authored records that still point at a source identity from the old revision.
///
/// The finding is advisory. It identifies a replacement candidate but never changes an
/// expectation, verification, decision, work record, or review anchor automatically.
#[must_use]
pub fn source_migration_findings(old: &ProjectAnalysis, new: &ProjectAnalysis) -> Vec<Finding> {
    let migrations = source_migrations(old, new);
    let by_old_id = migrations
        .iter()
        .map(|migration| (migration.old_id.as_str(), migration))
        .collect::<BTreeMap<_, _>>();
    let by_old_path = migrations
        .iter()
        .filter(|migration| migration.kind == "file")
        .map(|migration| (migration.old_path.as_str(), migration))
        .collect::<BTreeMap<_, _>>();
    let mut findings = Vec::new();

    for expectation in &new.expectations {
        if let Some(subject) = expectation.subject.as_deref()
            && let Some(migration) = by_old_id.get(subject)
        {
            findings.push(migration_finding(
                "expectation",
                &expectation.id,
                migration,
                new,
            ));
        }
    }
    for verification in &new.verifications {
        let Some(expectation) = new
            .expectations
            .iter()
            .find(|expectation| expectation.id == verification.expectation_id)
        else {
            continue;
        };
        if let Some(subject) = expectation.subject.as_deref()
            && let Some(migration) = by_old_id.get(subject)
        {
            findings.push(migration_finding(
                "verification",
                &verification.id,
                migration,
                new,
            ));
        }
    }
    for decision in &new.decisions {
        if let Some(subject) = decision.subject.as_deref()
            && let Some(migration) = by_old_id.get(subject)
        {
            findings.push(migration_finding("decision", &decision.id, migration, new));
        }
    }
    for work in &new.works {
        if let Some(subject) = work.subject.as_deref()
            && let Some(migration) = by_old_id.get(subject)
        {
            findings.push(migration_finding("work", &work.id, migration, new));
        }
    }
    for review in &new.review_threads {
        if let Some(subject) = review.subject.as_deref()
            && let Some(migration) = by_old_id.get(subject)
        {
            findings.push(migration_finding("review", &review.id, migration, new));
        } else if let Some(ReviewAnchor::Source { path, .. }) = review.anchor.as_ref()
            && let Some(migration) = by_old_path.get(path.as_str())
        {
            findings.push(migration_finding(
                "review anchor",
                &review.id,
                migration,
                new,
            ));
        }
    }
    findings
}

fn migration_finding(
    record_kind: &str,
    record_id: &str,
    migration: &SourceMigration,
    new: &ProjectAnalysis,
) -> Finding {
    let file_id = new
        .files
        .iter()
        .find(|file| file.path == migration.new_path)
        .map(|file| file.id.clone());
    Finding {
        rule_id: "SUS056".to_owned(),
        source: "susumu:derived".to_owned(),
        severity: Severity::Warning,
        title: "Authored source target may need migration".to_owned(),
        detail: format!(
            "{record_kind} `{record_id}` still references `{}` from an older source revision. Candidate replacement is `{}` in `{}` ({:?}); review and update the authored record explicitly.",
            migration.old_id, migration.new_id, migration.new_path, migration.confidence
        ),
        file_id,
        subject: Some(record_id.to_owned()),
        location: None,
    }
}

fn match_files<'a>(
    old: &'a [SourceFile],
    new: &'a [SourceFile],
) -> Vec<(&'a SourceFile, &'a SourceFile, MigrationConfidence)> {
    let mut matches = Vec::new();
    let mut used = BTreeSet::new();
    let new_by_id = new
        .iter()
        .map(|file| (file.id.as_str(), file))
        .collect::<BTreeMap<_, _>>();

    for old_file in old {
        if let Some(new_file) = new_by_id.get(old_file.id.as_str()) {
            matches.push((old_file, *new_file, MigrationConfidence::Exact));
            used.insert(new_file.id.as_str());
            continue;
        }
        let candidates = new
            .iter()
            .filter(|file| {
                !used.contains(file.id.as_str())
                    && file.language == old_file.language
                    && old_file.content_hash.is_some()
                    && old_file.content_hash == file.content_hash
            })
            .collect::<Vec<_>>();
        if candidates.len() == 1 {
            let new_file = candidates[0];
            used.insert(new_file.id.as_str());
            matches.push((old_file, new_file, MigrationConfidence::Exact));
        }
    }
    matches
}

fn same_symbol_shape(old: &Symbol, new: &Symbol) -> bool {
    old.kind == new.kind
        && (old.name == new.name
            || (old.content_hash.is_some() && old.content_hash == new.content_hash))
}

fn same_workflow_shape(old: &Workflow, new: &Workflow) -> bool {
    old.kind == new.kind
        && old.framework == new.framework
        && old.trigger == new.trigger
        && old.handler == new.handler
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Expectation, ExpectationStatus, ExpectationTarget, Language, Location, SymbolKind,
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
}
