#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) fn git_connect(args: &GitConnectArgs) -> Result<()> {
    let artifact = read_analysis_artifact(&args.artifact)?;
    run_git_connect(args, &artifact)
}

pub(crate) fn run_git_connect(args: &GitConnectArgs, artifact: &ProjectAnalysis) -> Result<()> {
    let commits = git_commits_for(
        &args.repo,
        args.since.as_deref(),
        args.until.as_deref(),
        args.limit,
    )?;
    let report = build_git_connect_report(artifact, &commits);
    let export = export_git_connect_work(args, artifact, &report)?;
    if args.json {
        print_git_connect_json(args, &report, export.as_ref())?;
    } else {
        print_git_connect_report(args, &report, export.as_ref());
    }
    Ok(())
}

pub(crate) fn git_link(args: &GitLinkArgs) -> Result<()> {
    let artifact = read_analysis_artifact(&args.artifact)?;
    let expectation = artifact
        .expectations
        .iter()
        .find(|expectation| expectation.id == args.expectation)
        .with_context(|| {
            format!(
                "{} does not contain expectation {}",
                args.artifact.display(),
                args.expectation
            )
        })?;
    let commit = git_commit_for_ref(&args.repo, &args.commit)?;
    let work = work_from_git_link(&commit, expectation, args);
    save_git_link_work(args, &work)?;
    print_git_link_result(args, &commit, expectation, &work)
}

pub(crate) fn save_git_link_work(args: &GitLinkArgs, work: &Work) -> Result<()> {
    let mut works = if args.output.exists() {
        read_work_sidecar(&args.output)?
    } else {
        Vec::new()
    };
    merge_works(&mut works, vec![work.clone()]);
    write_text_file(&args.output, &write_works(&works, args.minify)?)?;
    Ok(())
}

pub(crate) fn print_git_link_result(
    args: &GitLinkArgs,
    commit: &GitCommit,
    expectation: &Expectation,
    work: &Work,
) -> Result<()> {
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "output": args.output,
                "record": {
                    "id": work.id,
                    "commit": commit.hash,
                    "expectation": expectation.id,
                    "target": work.target.to_string(),
                    "subject": work.subject,
                    "kind": work.kind.to_string(),
                    "status": work.status.to_string(),
                    "evidence": work.evidence,
                    "title": work.title,
                }
            }))
            .context("could not serialize git link report")?
        );
    } else {
        let id = work.id.clone();
        println!("Susumu git link: {}", args.repo.display());
        println!("Commit: {}  {}", short_hash(&commit.hash), commit.subject);
        println!("Expectation: {}  {}", expectation.id, expectation.title);
        println!("Work: {id} -> {}", args.output.display());
    }

    Ok(())
}

pub(crate) fn import_git_work(args: &GitImportArgs) -> Result<()> {
    let commits = git_commits(args)?;
    let artifact = args
        .artifact
        .as_ref()
        .map(read_analysis_artifact)
        .transpose()?;
    let context = GitImportContext {
        artifact: artifact.as_ref(),
        target_depth: GitTargetDepth::from(args.target_depth.clone()),
    };
    let imported = commits
        .into_iter()
        .map(|commit| imported_git_work(&commit, &args.source, &context))
        .collect::<Vec<_>>();

    let mut works = if args.output.exists() {
        read_work_sidecar(&args.output)?
    } else {
        Vec::new()
    };
    let count = imported.len();
    merge_works(
        &mut works,
        imported
            .iter()
            .map(|imported| imported.work.clone())
            .collect(),
    );
    fs::write(&args.output, write_works(&works, args.minify)?)
        .with_context(|| format!("could not write {}", args.output.display()))?;
    if args.json {
        print_git_import_json(&args.output, &imported)?;
    } else {
        eprintln!(
            "imported {count} git commits into {}",
            args.output.display()
        );
    }
    Ok(())
}

pub(crate) fn export_git_connect_work(
    args: &GitConnectArgs,
    artifact: &ProjectAnalysis,
    report: &GitConnectReport,
) -> Result<Option<GitConnectExport>> {
    let Some(output) = &args.export_work else {
        return Ok(None);
    };

    let exported = report
        .records
        .iter()
        .filter(|connection| connection.status == "needs_record")
        .flat_map(|connection| works_from_git_connection(artifact, connection, &args.source))
        .collect::<Vec<_>>();
    let mut works = if output.exists() {
        read_work_sidecar(output)?
    } else {
        Vec::new()
    };
    let written = exported.len();
    merge_works(&mut works, exported);
    write_text_file(output, &write_works(&works, args.minify)?)?;
    Ok(Some(GitConnectExport {
        path: output.display().to_string(),
        written,
        source: args.source.clone(),
    }))
}

pub(crate) fn works_from_git_connection(
    artifact: &ProjectAnalysis,
    connection: &GitConnection,
    source: &str,
) -> Vec<Work> {
    let missing_expectations =
        missing_expectation_work_records(artifact, &connection.commit, &connection.expectations);
    if !missing_expectations.is_empty() {
        let preserve_single_id = connection.expectations.len() == 1;
        return missing_expectations
            .iter()
            .map(|expectation| {
                work_from_git_connection(
                    artifact,
                    connection,
                    source,
                    Some(expectation.id.as_str()),
                    preserve_single_id,
                )
            })
            .collect();
    }
    vec![work_from_git_connection(
        artifact, connection, source, None, true,
    )]
}

pub(crate) fn work_from_git_connection(
    artifact: &ProjectAnalysis,
    connection: &GitConnection,
    source: &str,
    expectation_id: Option<&str>,
    preserve_single_id: bool,
) -> Work {
    let (target, subject) = work_target_from_connection(artifact, connection, expectation_id);
    Work {
        id: git_connection_work_id(&connection.commit, expectation_id, preserve_single_id),
        target,
        subject,
        expectation_id: expectation_id.map(str::to_owned),
        kind: WorkKind::Implementation,
        status: WorkStatus::Completed,
        source: source.to_owned(),
        evidence: Some(format!("commit:{}", connection.commit)),
        title: connection.title.clone(),
        detail: git_connect_work_detail(connection),
    }
}

pub(crate) fn work_from_git_link(
    commit: &GitCommit,
    expectation: &Expectation,
    args: &GitLinkArgs,
) -> Work {
    Work {
        id: git_connection_work_id(&commit.hash, Some(expectation.id.as_str()), false),
        target: expectation.target,
        subject: expectation.subject.clone(),
        expectation_id: Some(expectation.id.clone()),
        kind: WorkKind::from(args.kind.clone()),
        status: WorkStatus::from(args.status.clone()),
        source: args.source.clone(),
        evidence: Some(format!("commit:{}", commit.hash)),
        title: args.title.clone().unwrap_or_else(|| commit.subject.clone()),
        detail: git_link_work_detail(commit, expectation, args.detail.as_deref()),
    }
}

pub(crate) fn work_target_from_connection(
    artifact: &ProjectAnalysis,
    connection: &GitConnection,
    expectation_id: Option<&str>,
) -> (ExpectationTarget, Option<String>) {
    if let Some(expectation_id) = expectation_id
        && let Some(expectation) = artifact
            .expectations
            .iter()
            .find(|expectation| expectation.id == expectation_id)
        && (expectation.target == ExpectationTarget::Project || expectation.subject.is_some())
    {
        return (expectation.target, expectation.subject.clone());
    }
    if connection.workflows.len() == 1 {
        return (
            ExpectationTarget::Workflow,
            Some(connection.workflows[0].id.clone()),
        );
    }
    (ExpectationTarget::Project, None)
}

pub(crate) fn git_connect_work_detail(connection: &GitConnection) -> String {
    let mut detail = format!(
        "Generated by git connect.\nCommit: {}\nAuthor: {}\nDate: {}\nStatus before export: {}\nReasons: {}",
        connection.commit,
        connection.author,
        connection.date,
        connection.status,
        connection.reasons.join("; ")
    );
    append_connected_records(&mut detail, "Workflows", &connection.workflows);
    append_connected_records(&mut detail, "Expectations", &connection.expectations);
    append_connected_records(&mut detail, "Verifications", &connection.verifications);
    append_connected_records(&mut detail, "Decisions", &connection.decisions);
    if !connection.changed_files.is_empty() {
        detail.push_str("\nChanged files:");
        for path in &connection.changed_files {
            detail.push_str("\n- ");
            detail.push_str(path);
        }
    }
    detail
}

pub(crate) fn git_link_work_detail(
    commit: &GitCommit,
    expectation: &Expectation,
    note: Option<&str>,
) -> String {
    let mut detail = format!(
        "Generated by git link.\nCommit: {}\nAuthor: {} <{}>\nDate: {}\nExpectation: {} - {}",
        commit.hash,
        commit.author_name,
        commit.author_email,
        commit.author_date,
        expectation.id,
        expectation.title
    );
    if !commit.changed_files.is_empty() {
        detail.push_str("\nChanged files:");
        for path in &commit.changed_files {
            detail.push_str("\n- ");
            detail.push_str(path);
        }
    }
    if let Some(note) = note.filter(|value| !value.trim().is_empty()) {
        detail.push_str("\n\nNote:\n");
        detail.push_str(note);
    }
    detail
}

pub(crate) fn append_connected_records(
    detail: &mut String,
    title: &str,
    records: &[GitConnectedRecord],
) {
    if records.is_empty() {
        return;
    }
    detail.push('\n');
    detail.push_str(title);
    detail.push(':');
    for record in records {
        detail.push_str("\n- ");
        detail.push_str(&record.id);
        detail.push_str(" - ");
        detail.push_str(&record.title);
        detail.push_str(" (");
        detail.push_str(&record.reason);
        detail.push(')');
    }
}

pub(crate) fn imported_git_work(
    commit: &GitCommit,
    source: &str,
    context: &GitImportContext<'_>,
) -> ImportedGitWork {
    let target = git_work_target(commit, context);
    let linked_expectation = linked_git_expectation(commit, context.artifact);
    let target = git_target_with_expectation(target, linked_expectation.as_ref());
    let targeting = target.note.clone();
    let work = Work {
        id: git_work_id(&commit.hash),
        target: target.target,
        subject: target.subject,
        expectation_id: linked_expectation.map(|expectation| expectation.id.clone()),
        kind: WorkKind::Implementation,
        status: WorkStatus::Completed,
        source: source.to_owned(),
        evidence: Some(format!("commit:{}", commit.hash)),
        title: commit.subject.clone(),
        detail: git_work_detail(commit, &target.note),
    };
    ImportedGitWork {
        work,
        commit_hash: commit.hash.clone(),
        targeting,
        changed_files: commit.changed_files.clone(),
    }
}

pub(crate) fn print_git_import_json(output: &Path, imported: &[ImportedGitWork]) -> Result<()> {
    let output = build_git_import_json(output, imported);
    println!(
        "{}",
        serde_json::to_string_pretty(&output).context("could not serialize git import report")?
    );
    Ok(())
}

pub(crate) fn build_git_import_json<'a>(
    output: &Path,
    imported: &'a [ImportedGitWork],
) -> GitImportJson<'a> {
    let records = imported
        .iter()
        .map(|imported| GitImportRecordJson {
            id: &imported.work.id,
            commit: &imported.commit_hash,
            target: imported.work.target.to_string(),
            subject: imported.work.subject.as_deref(),
            expectation: imported.work.expectation_id.as_deref(),
            title: &imported.work.title,
            targeting: &imported.targeting,
            changed_files: &imported.changed_files,
        })
        .collect::<Vec<_>>();
    GitImportJson {
        output: output.display().to_string(),
        imported: imported.len(),
        records,
    }
}

pub(crate) fn git_target_with_expectation(
    target: GitWorkTarget,
    expectation: Option<&GitExpectationLink>,
) -> GitWorkTarget {
    let Some(expectation) = expectation else {
        return target;
    };
    if target.target == ExpectationTarget::Project
        && expectation.target != ExpectationTarget::Project
        && expectation.subject.is_some()
    {
        return GitWorkTarget {
            target: expectation.target,
            subject: expectation.subject.clone(),
            note: format!(
                "{} Linked exact expectation id `{}` and used its target.",
                target.note, expectation.id
            ),
        };
    }
    GitWorkTarget {
        note: format!(
            "{} Linked exact expectation id `{}`.",
            target.note, expectation.id
        ),
        ..target
    }
}

pub(crate) fn linked_git_expectation(
    commit: &GitCommit,
    artifact: Option<&ProjectAnalysis>,
) -> Option<GitExpectationLink> {
    let artifact = artifact?;
    let searchable = format!("{}\n{}", commit.subject, commit.body);
    let mut matches = explicitly_linked_git_expectations(artifact, &searchable);
    if matches.is_empty()
        && let Some(expectation_id) = single_language_matched_expectation(artifact, &searchable)
        && let Some(expectation) = artifact
            .expectations
            .iter()
            .find(|expectation| expectation.id == expectation_id)
    {
        matches.push(GitExpectationLink {
            id: expectation.id.clone(),
            target: expectation.target,
            subject: expectation.subject.clone(),
        });
    }
    matches.sort_by(|left, right| left.id.cmp(&right.id));
    matches.dedup_by(|left, right| left.id == right.id);
    (matches.len() == 1).then(|| matches.remove(0))
}

pub(crate) fn explicitly_linked_git_expectations(
    artifact: &ProjectAnalysis,
    searchable: &str,
) -> Vec<GitExpectationLink> {
    artifact
        .expectations
        .iter()
        .filter(|expectation| contains_token(searchable, &expectation.id))
        .map(|expectation| GitExpectationLink {
            id: expectation.id.clone(),
            target: expectation.target,
            subject: expectation.subject.clone(),
        })
        .collect::<Vec<_>>()
}

pub(crate) fn git_work_target(commit: &GitCommit, context: &GitImportContext<'_>) -> GitWorkTarget {
    let Some(artifact) = context.artifact else {
        return project_git_target("No artifact supplied; imported as project-wide work.");
    };
    if context.target_depth == GitTargetDepth::Project {
        return project_git_target("Target depth is project.");
    }

    let matched_file_ids = matched_artifact_file_ids(artifact, &commit.changed_files);
    if context.target_depth == GitTargetDepth::Workflow
        && let Some(workflow_id) = single_workflow_for_files(artifact, &matched_file_ids)
    {
        return GitWorkTarget {
            target: ExpectationTarget::Workflow,
            subject: Some(workflow_id),
            note: "Matched exactly one workflow from changed files.".to_owned(),
        };
    }

    if matched_file_ids.len() == 1 {
        return GitWorkTarget {
            target: ExpectationTarget::File,
            subject: matched_file_ids.first().cloned(),
            note: "Matched exactly one artifact file from changed files.".to_owned(),
        };
    }

    project_git_target(if matched_file_ids.is_empty() {
        "Changed files did not match artifact files."
    } else {
        "Changed files matched multiple artifact files."
    })
}

pub(crate) fn project_git_target(note: &str) -> GitWorkTarget {
    GitWorkTarget {
        target: ExpectationTarget::Project,
        subject: None,
        note: note.to_owned(),
    }
}

pub(crate) fn single_workflow_for_files(
    artifact: &ProjectAnalysis,
    file_ids: &[String],
) -> Option<String> {
    let mut workflows = artifact
        .workflows
        .iter()
        .filter(|workflow| file_ids.iter().any(|file_id| file_id == &workflow.file_id))
        .map(|workflow| workflow.id.clone())
        .collect::<Vec<_>>();
    workflows.sort();
    workflows.dedup();
    (workflows.len() == 1).then(|| workflows.remove(0))
}

pub(crate) fn git_work_id(hash: &str) -> String {
    let short = hash.chars().take(16).collect::<String>();
    format!("wk_git_{short}")
}

pub(crate) fn git_connection_work_id(
    hash: &str,
    expectation_id: Option<&str>,
    preserve_single_id: bool,
) -> String {
    if preserve_single_id {
        return git_work_id(hash);
    }
    let Some(expectation_id) = expectation_id else {
        return git_work_id(hash);
    };
    let mut digest = Sha256::new();
    digest.update(hash.as_bytes());
    digest.update([0]);
    digest.update(expectation_id.as_bytes());
    let suffix = hex_prefix(&digest.finalize(), 4);
    format!("{}_{}", git_work_id(hash), suffix)
}

pub(crate) fn short_hash(hash: &str) -> String {
    hash.chars().take(8).collect()
}

pub(crate) fn git_work_detail(commit: &GitCommit, target_note: &str) -> String {
    let mut detail = format!(
        "Author: {} <{}>\nDate: {}\nCommit: {}\nTargeting: {}",
        commit.author_name, commit.author_email, commit.author_date, commit.hash, target_note
    );
    if !commit.changed_files.is_empty() {
        detail.push_str("\nChanged files:");
        for path in &commit.changed_files {
            detail.push_str("\n- ");
            detail.push_str(path);
        }
    }
    if !commit.body.is_empty() {
        detail.push_str("\n\n");
        detail.push_str(&commit.body);
    }
    detail
}
