#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) fn init_repository(args: &InitArgs) -> Result<()> {
    if !args.target.is_dir() {
        bail!("{} is not a directory", args.target.display());
    }

    let file = if args.file.is_absolute() {
        args.file.clone()
    } else {
        args.target.join(&args.file)
    };

    if file.exists() && !args.force {
        bail!(
            "{} already exists; use --force to replace it",
            file.display()
        );
    }

    if let Some(parent) = file.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }

    let project_name = args.name.clone().unwrap_or_else(|| {
        args.target
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("project")
            .to_owned()
    });
    let expectations = starter_expectations(&project_name, &args.source);
    fs::write(&file, write_expectations(&expectations, false)?)
        .with_context(|| format!("could not write {}", file.display()))?;

    eprintln!(
        "wrote {} starter expectations to {}",
        expectations.len(),
        file.display()
    );
    eprintln!("next: susumu review {}", args.target.display());
    Ok(())
}

fn starter_expectations(project_name: &str, source: &str) -> Vec<Expectation> {
    let records = [
        (
            ExpectationStatus::Accepted,
            format!("{project_name} keeps expectations explicit"),
            format!(
                "{project_name} should keep project expectations in an authored expectations.susu sidecar so implementation evidence and intent can be reviewed together."
            ),
        ),
        (
            ExpectationStatus::Proposed,
            format!("{project_name} documents primary workflows"),
            format!(
                "{project_name} should describe the business or product workflows that matter most, then link those expectations to observed files, symbols, or workflows as evidence improves."
            ),
        ),
        (
            ExpectationStatus::Proposed,
            format!("{project_name} records verification evidence"),
            format!(
                "{project_name} should record how important expectations are checked, such as tests, CI runs, manual reviews, policy checks, runtime traces, or release approvals."
            ),
        ),
    ];

    records
        .into_iter()
        .map(|(status, title, detail)| {
            let id = expectation_id(
                ExpectationTarget::Project,
                None,
                status,
                source,
                &title,
                &detail,
            );
            Expectation {
                id,
                target: ExpectationTarget::Project,
                subject: None,
                status,
                source: source.to_owned(),
                title,
                detail,
            }
        })
        .collect()
}

pub(crate) fn write_text_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("could not write {}", path.display()))
}

pub(crate) fn check(args: &CheckArgs) -> Result<()> {
    let analysis = load_analysis(
        &args.target,
        args.expectations.as_ref(),
        args.verifications.as_ref(),
        args.decisions.as_ref(),
        args.work.as_ref(),
        false,
    )?;
    let report = check_report(&analysis, args.strict);
    if args.json {
        print_check_json(&analysis, &report)?;
    } else {
        print_check_report(&analysis, &report, args.max_items);
    }
    if report.failed {
        process::exit(1);
    }
    Ok(())
}

pub(crate) fn diff(args: &DiffArgs) -> Result<()> {
    let old = read_analysis_artifact(&args.old)?;
    let new = read_analysis_artifact(&args.new)?;
    let report = diff_report(&old, &new);
    if args.json {
        print_diff_json(&old, &new, &report, args.fail_on_stale)?;
    } else {
        print_diff_report(&old, &new, &report, args.max_items);
    }
    if args.fail_on_stale && !report.stale_items.is_empty() {
        process::exit(1);
    }
    Ok(())
}

pub(crate) fn handoff(args: &HandoffArgs) -> Result<()> {
    let analysis = load_analysis(
        &args.target,
        args.expectations.as_ref(),
        args.verifications.as_ref(),
        args.decisions.as_ref(),
        args.work.as_ref(),
        false,
    )?;
    let check = check_report(&analysis, false);
    let report = handoff_report(&analysis, &check);
    if args.json {
        print_handoff_json(&analysis, &check, &report)?;
    } else {
        print_handoff_report(&analysis, &check, &report, args.max_items);
    }
    Ok(())
}

pub(crate) fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

pub(crate) fn expectation_title(analysis: &ProjectAnalysis, id: &str) -> String {
    analysis
        .expectations
        .iter()
        .find(|expectation| expectation.id == id)
        .map_or_else(|| id.to_owned(), |expectation| expectation.title.clone())
}
