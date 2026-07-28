#![allow(clippy::wildcard_imports)]
use super::*;

pub(super) fn git_rewind(args: &GitRewindArgs) -> Result<()> {
    let snapshot_dir = git_snapshot_dir(&args.repo, &args.from)?;
    let result = execute_git_rewind(args, &snapshot_dir);
    cleanup_git_snapshot(&snapshot_dir);
    let failed = result?;
    if failed {
        process::exit(1);
    }
    Ok(())
}

fn execute_git_rewind(args: &GitRewindArgs, snapshot_dir: &Path) -> Result<bool> {
    let mut old = scan_project(snapshot_dir)
        .with_context(|| format!("could not scan Git ref {}", args.from))?;
    old.project_name = format!("{}@{}", git_repo_label(&args.repo), args.from);
    let new = if let Some(artifact) = &args.artifact {
        read_analysis_artifact(artifact)?
    } else {
        load_analysis(&args.repo, None, None, None, None, None, false)?
    };
    if let Some(output) = &args.old_output {
        fs::write(output, write_susu(&old, args.minify)?)
            .with_context(|| format!("could not write {}", output.display()))?;
        eprintln!("wrote old-ref artifact {}", output.display());
    }

    let report = diff_report(&old, &new);
    if args.json {
        print_git_rewind_json(args, &old, &new, &report)?;
    } else {
        println!(
            "Susumu git rewind: {}@{} -> {}",
            args.repo.display(),
            args.from,
            args.artifact.as_ref().map_or_else(
                || args.repo.display().to_string(),
                |path| path.display().to_string()
            )
        );
        println!();
        print_diff_report(&old, &new, &report, args.max_items);
    }
    Ok(args.fail_on_stale && !report.stale_items.is_empty())
}

fn cleanup_git_snapshot(snapshot_dir: &Path) {
    if let Err(error) = fs::remove_dir_all(snapshot_dir) {
        eprintln!(
            "warning: could not remove temporary Git snapshot {}: {error}",
            snapshot_dir.display()
        );
    }
}

pub(super) fn read_analysis_artifact(path: &PathBuf) -> Result<ProjectAnalysis> {
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("susu"))
    {
        bail!("{} is not a .susu artifact", path.display());
    }
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    let mut analysis =
        parse_susu(&source).with_context(|| format!("could not parse {}", path.display()))?;
    refresh_derived_analysis(&mut analysis);
    Ok(analysis)
}

#[derive(Debug, Default)]
pub(super) struct ChangeSummary {
    pub(super) added: Vec<String>,
    pub(super) removed: Vec<String>,
    pub(super) changed: Vec<String>,
}

impl ChangeSummary {
    fn total(&self) -> usize {
        self.added.len() + self.removed.len() + self.changed.len()
    }
}

#[derive(Debug)]
pub(super) struct DiffReport {
    pub(super) files: ChangeSummary,
    pub(super) workflows: ChangeSummary,
    pub(super) expectations: ChangeSummary,
    pub(super) verifications: ChangeSummary,
    pub(super) decisions: ChangeSummary,
    pub(super) works: ChangeSummary,
    pub(super) stale_items: Vec<CheckItem>,
}

#[derive(Debug, Serialize)]
struct DiffJson<'a> {
    old: DiffProjectJson<'a>,
    new: DiffProjectJson<'a>,
    changes: DiffChangesJson<'a>,
    freshness: DiffFreshnessJson<'a>,
    result: DiffResultJson<'a>,
}

#[derive(Debug, Serialize)]
struct DiffProjectJson<'a> {
    name: &'a str,
    root: &'a str,
    generated_unix_seconds: u64,
}

#[derive(Debug, Serialize)]
struct DiffChangesJson<'a> {
    files: ChangeSummaryJson<'a>,
    workflows: ChangeSummaryJson<'a>,
    expectations: ChangeSummaryJson<'a>,
    verifications: ChangeSummaryJson<'a>,
    decisions: ChangeSummaryJson<'a>,
    work: ChangeSummaryJson<'a>,
}

#[derive(Debug, Serialize)]
pub(super) struct ChangeSummaryJson<'a> {
    added: &'a [String],
    removed: &'a [String],
    changed: &'a [String],
}

#[derive(Debug, Serialize)]
struct DiffFreshnessJson<'a> {
    stale: usize,
    items: Vec<CheckItemJson<'a>>,
}

#[derive(Debug, Serialize)]
struct DiffResultJson<'a> {
    status: &'a str,
    failed: bool,
    fail_on_stale: bool,
    reason: &'a str,
}

#[derive(Debug, Serialize)]
struct GitRewindJson<'a> {
    git: GitRewindGitJson,
    old: DiffProjectJson<'a>,
    new: DiffProjectJson<'a>,
    changes: DiffChangesJson<'a>,
    freshness: DiffFreshnessJson<'a>,
    result: DiffResultJson<'a>,
}

#[derive(Debug, Serialize)]
struct GitRewindGitJson {
    repo: String,
    from: String,
    artifact: Option<String>,
    old_output: Option<String>,
}

pub(super) fn diff_report(old: &ProjectAnalysis, new: &ProjectAnalysis) -> DiffReport {
    DiffReport {
        files: diff_by(
            &old.files,
            &new.files,
            |file| file.path.clone(),
            |file| file.path.clone(),
        ),
        workflows: diff_by(
            &old.workflows,
            &new.workflows,
            |workflow| format!("{}|{}", workflow.framework, workflow.trigger),
            |workflow| format!("{} ({})", workflow.trigger, workflow.framework),
        ),
        expectations: diff_by(
            &old.expectations,
            &new.expectations,
            |expectation| expectation.id.clone(),
            |expectation| format!("{} - {}", expectation.id, expectation.title),
        ),
        verifications: diff_by(
            &old.verifications,
            &new.verifications,
            |verification| verification.id.clone(),
            |verification| {
                format!(
                    "{} - {}",
                    verification.id,
                    expectation_title(new, &verification.expectation_id)
                )
            },
        ),
        decisions: diff_by(
            &old.decisions,
            &new.decisions,
            |decision| decision.id.clone(),
            |decision| format!("{} - {}", decision.id, decision.title),
        ),
        works: diff_by(
            &old.works,
            &new.works,
            |work| work.id.clone(),
            |work| format!("{} - {}", work.id, work.title),
        ),
        stale_items: freshness_check_items(new),
    }
}

pub(super) fn diff_by<T>(
    old: &[T],
    new: &[T],
    key: impl Fn(&T) -> String,
    label: impl Fn(&T) -> String,
) -> ChangeSummary
where
    T: PartialEq,
{
    let old_map = old
        .iter()
        .map(|item| (key(item), item))
        .collect::<BTreeMap<_, _>>();
    let new_map = new
        .iter()
        .map(|item| (key(item), item))
        .collect::<BTreeMap<_, _>>();
    let mut summary = ChangeSummary::default();

    for (item_key, new_item) in &new_map {
        match old_map.get(item_key) {
            Some(old_item) if *old_item != *new_item => summary.changed.push(label(new_item)),
            Some(_) => {}
            None => summary.added.push(label(new_item)),
        }
    }
    for (item_key, old_item) in &old_map {
        if !new_map.contains_key(item_key) {
            summary.removed.push(label(old_item));
        }
    }

    summary
}

fn freshness_check_items(analysis: &ProjectAnalysis) -> Vec<CheckItem> {
    analysis
        .findings
        .iter()
        .filter(|finding| matches!(finding.rule_id.as_str(), "SUS023" | "SUS033"))
        .map(|finding| CheckItem {
            severity: CheckSeverity::Warning,
            title: format!("{}: {}", finding.rule_id, finding.title),
            detail: finding.detail.clone(),
            source: finding.source.clone(),
        })
        .collect()
}

pub(super) fn print_diff_report(
    old: &ProjectAnalysis,
    new: &ProjectAnalysis,
    report: &DiffReport,
    max_items: usize,
) {
    println!("Susumu diff: {} -> {}", old.project_name, new.project_name);
    println!("Old generated: {}", old.generated_unix_seconds);
    println!("New generated: {}", new.generated_unix_seconds);
    println!();
    print_change_section("Files", &report.files, max_items);
    print_change_section("Workflows", &report.workflows, max_items);
    print_change_section("Expectations", &report.expectations, max_items);
    print_change_section("Verifications", &report.verifications, max_items);
    print_change_section("Decisions", &report.decisions, max_items);
    print_change_section("Work", &report.works, max_items);
    print_freshness_section(&report.stale_items, max_items);
}

pub(super) fn print_diff_json(
    old: &ProjectAnalysis,
    new: &ProjectAnalysis,
    report: &DiffReport,
    fail_on_stale: bool,
) -> Result<()> {
    let failed = fail_on_stale && !report.stale_items.is_empty();
    let output = DiffJson {
        old: DiffProjectJson {
            name: &old.project_name,
            root: &old.root,
            generated_unix_seconds: old.generated_unix_seconds,
        },
        new: DiffProjectJson {
            name: &new.project_name,
            root: &new.root,
            generated_unix_seconds: new.generated_unix_seconds,
        },
        changes: DiffChangesJson {
            files: change_summary_json(&report.files),
            workflows: change_summary_json(&report.workflows),
            expectations: change_summary_json(&report.expectations),
            verifications: change_summary_json(&report.verifications),
            decisions: change_summary_json(&report.decisions),
            work: change_summary_json(&report.works),
        },
        freshness: DiffFreshnessJson {
            stale: report.stale_items.len(),
            items: check_item_jsons(&report.stale_items),
        },
        result: DiffResultJson {
            status: if failed { "failed" } else { "passed" },
            failed,
            fail_on_stale,
            reason: diff_result_reason(failed, report.stale_items.len()),
        },
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&output).context("could not serialize diff report")?
    );
    Ok(())
}

fn print_git_rewind_json(
    args: &GitRewindArgs,
    old: &ProjectAnalysis,
    new: &ProjectAnalysis,
    report: &DiffReport,
) -> Result<()> {
    let failed = args.fail_on_stale && !report.stale_items.is_empty();
    let output = GitRewindJson {
        git: GitRewindGitJson {
            repo: args.repo.display().to_string(),
            from: args.from.clone(),
            artifact: args
                .artifact
                .as_ref()
                .map(|path| path.display().to_string()),
            old_output: args
                .old_output
                .as_ref()
                .map(|path| path.display().to_string()),
        },
        old: DiffProjectJson {
            name: &old.project_name,
            root: &old.root,
            generated_unix_seconds: old.generated_unix_seconds,
        },
        new: DiffProjectJson {
            name: &new.project_name,
            root: &new.root,
            generated_unix_seconds: new.generated_unix_seconds,
        },
        changes: DiffChangesJson {
            files: change_summary_json(&report.files),
            workflows: change_summary_json(&report.workflows),
            expectations: change_summary_json(&report.expectations),
            verifications: change_summary_json(&report.verifications),
            decisions: change_summary_json(&report.decisions),
            work: change_summary_json(&report.works),
        },
        freshness: DiffFreshnessJson {
            stale: report.stale_items.len(),
            items: check_item_jsons(&report.stale_items),
        },
        result: DiffResultJson {
            status: if failed { "failed" } else { "passed" },
            failed,
            fail_on_stale: args.fail_on_stale,
            reason: diff_result_reason(failed, report.stale_items.len()),
        },
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&output).context("could not serialize rewind report")?
    );
    Ok(())
}

pub(super) fn change_summary_json(summary: &ChangeSummary) -> ChangeSummaryJson<'_> {
    ChangeSummaryJson {
        added: &summary.added,
        removed: &summary.removed,
        changed: &summary.changed,
    }
}

const fn diff_result_reason(failed: bool, stale: usize) -> &'static str {
    if failed {
        "stale evidence present"
    } else if stale > 0 {
        "passed with stale evidence"
    } else {
        "passed"
    }
}

pub(super) fn print_change_section(title: &str, summary: &ChangeSummary, max_items: usize) {
    println!("{title}:");
    println!("  added: {}", summary.added.len());
    println!("  removed: {}", summary.removed.len());
    println!("  changed: {}", summary.changed.len());
    if summary.total() > 0 {
        print_labeled_items("+", &summary.added, max_items);
        print_labeled_items("-", &summary.removed, max_items);
        print_labeled_items("~", &summary.changed, max_items);
    }
    println!();
}

fn print_labeled_items(prefix: &str, items: &[String], max_items: usize) {
    for item in items.iter().take(max_items) {
        println!("  {prefix} {item}");
    }
    if items.len() > max_items {
        println!("  ... {} more", items.len() - max_items);
    }
}

pub(super) fn print_freshness_section(items: &[CheckItem], max_items: usize) {
    println!("Freshness:");
    println!("  stale: {}", items.len());
    for item in items.iter().take(max_items) {
        println!("  [warning] {}", item.title);
        println!("      source={}", item.source);
        println!("      {}", item.detail);
    }
    if items.len() > max_items {
        println!("  ... {} more", items.len() - max_items);
    }
    println!();
}

#[derive(Debug)]
pub(super) struct ReviewDiffReport {
    pub(super) artifact: DiffReport,
    pub(super) review_items: ChangeSummary,
    pub(super) next_actions: ChangeSummary,
    pub(super) top_workflows: ChangeSummary,
}
