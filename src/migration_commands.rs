use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use susumu::{
    migration::{MigrationRewriteSummary, apply_accepted_migrations, source_migrations},
    model::{ExpectationTarget, Work, WorkKind, WorkStatus},
};

use crate::{
    MigrateArgs, current_unix_seconds, diff_commands::read_analysis_artifact, parse_decisions,
    parse_expectations, parse_review_threads, parse_works, write_decisions, write_expectations,
    write_review_threads, write_susu, write_works,
};

#[derive(Debug, Serialize)]
struct MigrationActionJson {
    old_id: String,
    new_id: Option<String>,
    disposition: String,
}

#[derive(Debug, Serialize)]
struct MigrationReportJson {
    old: PathBuf,
    new: PathBuf,
    candidates: Vec<susumu::migration::SourceMigration>,
    actions: Vec<MigrationActionJson>,
    rewritten: MigrationRewriteSummaryJson,
}

#[derive(Debug, Serialize)]
struct MigrationRewriteSummaryJson {
    expectations: usize,
    decisions: usize,
    works: usize,
    reviews: usize,
    anchors: usize,
}

pub(crate) fn run_migrate(args: &MigrateArgs) -> Result<()> {
    let old = read_analysis_artifact(&args.old)?;
    let mut current = read_analysis_artifact(&args.new)?;
    let candidates = source_migrations(&old, &current);
    let by_old = candidates
        .iter()
        .map(|candidate| (candidate.old_id.as_str(), candidate))
        .collect::<BTreeMap<_, _>>();
    let accepts = parse_accepts(&args.accepts, &by_old)?;
    let rejects = parse_dispositions(&args.rejects, &by_old, &accepts, "reject")?;
    let defers = parse_dispositions(&args.defers, &by_old, &accepts, "defer")?;

    let mut actions = Vec::new();
    let mut action_ids = BTreeSet::new();
    for candidate in &candidates {
        if let Some(new_id) = accepts.get(candidate.old_id.as_str()) {
            actions.push(MigrationActionJson {
                old_id: candidate.old_id.clone(),
                new_id: Some(new_id.clone()),
                disposition: "accept".to_owned(),
            });
            action_ids.insert(candidate.old_id.as_str());
        } else if rejects.contains(candidate.old_id.as_str()) {
            actions.push(MigrationActionJson {
                old_id: candidate.old_id.clone(),
                new_id: None,
                disposition: "reject".to_owned(),
            });
            action_ids.insert(candidate.old_id.as_str());
        } else if defers.contains(candidate.old_id.as_str()) {
            actions.push(MigrationActionJson {
                old_id: candidate.old_id.clone(),
                new_id: None,
                disposition: "defer".to_owned(),
            });
            action_ids.insert(candidate.old_id.as_str());
        }
    }

    let rewritten = if accepts.is_empty() {
        MigrationRewriteSummary::default()
    } else {
        let summary = apply_accepted_migrations(&mut current, &candidates, &accepts);
        update_sidecars(args, &accepts, &candidates)?;
        if let Some(output) = args.output.as_ref() {
            fs::write(output, write_susu(&current, false)?)
                .with_context(|| format!("could not write {}", output.display()))?;
        }
        summary
    };

    if !action_ids.is_empty() {
        append_audit_work(args.work.as_ref(), &current, &actions)?;
    }

    if args.json {
        let report = MigrationReportJson {
            old: args.old.clone(),
            new: args.new.clone(),
            candidates,
            actions,
            rewritten: rewritten.into(),
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_report(&candidates, &actions, rewritten, args.output.as_ref());
    }
    Ok(())
}

fn parse_accepts(
    values: &[String],
    candidates: &BTreeMap<&str, &susumu::migration::SourceMigration>,
) -> Result<BTreeMap<String, String>> {
    let mut accepts = BTreeMap::new();
    for value in values {
        let (old, new) = value
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--accept must use OLD_ID=NEW_ID"))?;
        let candidate = candidates
            .get(old)
            .ok_or_else(|| anyhow::anyhow!("no migration candidate has old id `{old}`"))?;
        if candidate.new_id != new {
            bail!("`{old}` can only be accepted as `{}`", candidate.new_id);
        }
        accepts.insert(old.to_owned(), new.to_owned());
    }
    Ok(accepts)
}

fn parse_dispositions(
    values: &[String],
    candidates: &BTreeMap<&str, &susumu::migration::SourceMigration>,
    accepts: &BTreeMap<String, String>,
    label: &str,
) -> Result<BTreeSet<String>> {
    let mut ids = BTreeSet::new();
    for value in values {
        if !candidates.contains_key(value.as_str()) {
            bail!("no migration candidate has old id `{value}`");
        }
        if accepts.contains_key(value) {
            bail!("migration `{value}` cannot be both accepted and {label}d");
        }
        ids.insert(value.clone());
    }
    Ok(ids)
}

fn update_sidecars(
    args: &MigrateArgs,
    accepted: &BTreeMap<String, String>,
    candidates: &[susumu::migration::SourceMigration],
) -> Result<()> {
    let mut analysis = read_analysis_artifact(&args.new)?;
    let summary = apply_accepted_migrations(&mut analysis, candidates, accepted);
    if let Some(path) = args.expectations.as_ref() {
        let source = fs::read_to_string(path)?;
        let mut records = parse_expectations(&source)?;
        for record in &mut records {
            if let Some(subject) = record.subject.as_mut()
                && let Some(replacement) = accepted.get(subject)
            {
                *subject = replacement.clone();
            }
        }
        fs::write(path, write_expectations(&records, false)?)?;
    }
    if let Some(path) = args.decisions.as_ref() {
        let source = fs::read_to_string(path)?;
        let mut records = parse_decisions(&source)?;
        for record in &mut records {
            if let Some(subject) = record.subject.as_mut()
                && let Some(replacement) = accepted.get(subject)
            {
                *subject = replacement.clone();
            }
        }
        fs::write(path, write_decisions(&records, false)?)?;
    }
    if let Some(path) = args.reviews.as_ref() {
        let source = fs::read_to_string(path)?;
        let mut records = parse_review_threads(&source)?;
        for record in &mut records {
            if let Some(subject) = record.subject.as_mut()
                && let Some(replacement) = accepted.get(subject)
            {
                *subject = replacement.clone();
            }
            if let Some(susumu::model::ReviewAnchor::Source { path, .. }) = record.anchor.as_mut()
                && let Some(migration) = candidates.iter().find(|migration| {
                    migration.kind == "file"
                        && migration.old_path == *path
                        && accepted.get(&migration.old_id) == Some(&migration.new_id)
                })
            {
                *path = migration.new_path.clone();
            }
        }
        fs::write(path, write_review_threads(&records, false)?)?;
    }
    if let Some(path) = args.work.as_ref() {
        let source = if path.exists() {
            fs::read_to_string(path)?
        } else {
            String::new()
        };
        let mut records = if source.trim().is_empty() {
            Vec::new()
        } else {
            parse_works(&source)?
        };
        for record in &mut records {
            if let Some(subject) = record.subject.as_mut()
                && let Some(replacement) = accepted.get(subject)
            {
                *subject = replacement.clone();
            }
        }
        fs::write(path, write_works(&records, false)?)?;
    }
    let _ = summary;
    Ok(())
}

fn append_audit_work(
    path: Option<&PathBuf>,
    analysis: &susumu::model::ProjectAnalysis,
    actions: &[MigrationActionJson],
) -> Result<()> {
    let Some(path) = path else { return Ok(()) };
    let source = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };
    let mut records = if source.trim().is_empty() {
        Vec::new()
    } else {
        parse_works(&source)?
    };
    let detail = actions
        .iter()
        .map(|action| match action.new_id.as_deref() {
            Some(new_id) => format!("{} {} -> {}", action.disposition, action.old_id, new_id),
            None => format!("{} {}", action.disposition, action.old_id),
        })
        .collect::<Vec<_>>()
        .join("; ");
    records.push(Work {
        id: format!(
            "migration-{}-{}",
            analysis.source_revision.as_deref().unwrap_or("unknown"),
            current_unix_seconds()
        ),
        target: ExpectationTarget::Project,
        subject: None,
        expectation_id: None,
        kind: WorkKind::Review,
        status: WorkStatus::Completed,
        source: "human:migration-review".to_owned(),
        evidence: analysis.source_revision.clone(),
        title: "Reviewed source migration candidates".to_owned(),
        detail,
    });
    fs::write(path, write_works(&records, false)?)?;
    Ok(())
}

fn print_report(
    candidates: &[susumu::migration::SourceMigration],
    actions: &[MigrationActionJson],
    rewritten: MigrationRewriteSummary,
    output: Option<&PathBuf>,
) {
    if candidates.is_empty() {
        println!("No source migration candidates.");
        return;
    }
    for candidate in candidates {
        println!(
            "{} -> {} [{:?}] {}",
            candidate.old_id, candidate.new_id, candidate.confidence, candidate.detail
        );
    }
    if actions.is_empty() {
        println!(
            "No dispositions recorded. Use --accept OLD_ID=NEW_ID, --reject OLD_ID, or --defer OLD_ID."
        );
    } else {
        println!(
            "Recorded {} disposition(s). Rewritten: expectations={}, decisions={}, work={}, reviews={}, anchors={}.{}",
            actions.len(),
            rewritten.expectations,
            rewritten.decisions,
            rewritten.works,
            rewritten.reviews,
            rewritten.anchors,
            output
                .map(|path| format!(" Wrote {}.", path.display()))
                .unwrap_or_default()
        );
    }
}

impl From<MigrationRewriteSummary> for MigrationRewriteSummaryJson {
    fn from(summary: MigrationRewriteSummary) -> Self {
        Self {
            expectations: summary.expectations,
            decisions: summary.decisions,
            works: summary.works,
            reviews: summary.reviews,
            anchors: summary.anchors,
        }
    }
}
