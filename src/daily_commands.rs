#![allow(clippy::wildcard_imports)]

use super::*;

#[derive(Debug)]
pub(super) struct DailyReviewPaths {
    artifact: PathBuf,
    packet: PathBuf,
    check_json: PathBuf,
    html: PathBuf,
    work: PathBuf,
}

#[derive(Debug)]
pub(super) struct ReviewBuildState {
    pub(super) project_name: String,
    pub(super) check: CheckReport,
}

pub(super) fn review_shortcut(args: &ReviewShortcutArgs) -> Result<()> {
    let paths = daily_review_paths(&args.target, &args.output_dir);
    let work = args
        .work
        .clone()
        .or_else(|| paths.work.exists().then_some(paths.work.clone()));
    build_review(&ReviewBuildArgs {
        target: args.target.clone(),
        expectations: None,
        verifications: None,
        decisions: None,
        work,
        artifact_output: paths.artifact,
        output: paths.packet,
        check_json: Some(paths.check_json),
        html: (!args.no_html).then_some(paths.html),
        strict: args.strict,
        fail_on_check: args.fail_on_check,
        json: args.json,
        serve: args.serve,
        host: args.host.clone(),
        port: args.port,
    })
}

pub(super) fn open_shortcut(args: &OpenArgs) -> Result<()> {
    if args.summary || args.tui || args.json {
        return open_review(&ReviewOpenArgs {
            packet: args.packet.clone(),
            max_items: args.max_items,
            json: args.json,
            tui: args.tui,
        });
    }

    if args.serve {
        return serve_review(&ReviewServeArgs {
            packet: args.packet.clone(),
            host: args.host.clone(),
            port: args.port,
        });
    }

    open_static_review(&args.packet)
}

pub(super) fn open_static_review(packet: &Path) -> Result<()> {
    let html = packet.with_extension("html");
    if !html.is_file() {
        bail!(
            "could not find static review portal at {}; run `susumu review` first",
            html.display()
        );
    }

    let status = if cfg!(target_os = "windows") {
        ProcessCommand::new("explorer.exe").arg(&html).status()
    } else if cfg!(target_os = "macos") {
        ProcessCommand::new("open").arg(&html).status()
    } else {
        ProcessCommand::new("xdg-open").arg(&html).status()
    }
    .with_context(|| format!("could not open static review portal {}", html.display()))?;

    if !status.success() {
        bail!(
            "could not open static review portal {} (browser launcher exited with {})",
            html.display(),
            status
        );
    }

    println!("Opened static review portal: {}", html.display());
    Ok(())
}

pub(super) fn status_shortcut(args: &StatusArgs) -> Result<()> {
    let paths = daily_review_paths(&args.target, &args.output_dir);
    let work = paths.work.exists().then_some(paths.work);
    check(&CheckArgs {
        target: args.target.clone(),
        expectations: None,
        verifications: None,
        decisions: None,
        work,
        strict: args.strict,
        max_items: args.max_items,
        json: args.json,
    })
}

#[derive(Debug, Serialize)]
pub(super) struct ExpectationsJson {
    source: String,
    total: usize,
    shown: usize,
    search: Option<String>,
    status: Option<String>,
    expectations: Vec<ExpectationBrowseRow>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct ExpectationBrowseRow {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) detail: String,
    pub(super) target: String,
    pub(super) subject: Option<String>,
    pub(super) status: String,
    pub(super) source: String,
    pub(super) support_status: Option<String>,
    pub(super) target_observed: Option<bool>,
    pub(super) verification: Option<ExpectationVerificationSupport>,
    pub(super) work: Option<usize>,
    pub(super) decisions: Option<usize>,
    pub(super) findings: Option<usize>,
}

pub(super) fn expectations_shortcut(args: &ExpectationsArgs) -> Result<()> {
    let (source, rows) = expectation_browse_rows(args)?;
    let status = args.status.clone().map(ExpectationStatus::from);
    let filtered = filter_expectation_rows(
        rows,
        args.search.as_deref(),
        status.as_ref().map(ToString::to_string).as_deref(),
    );
    let shown = filtered
        .iter()
        .take(args.max_items)
        .cloned()
        .collect::<Vec<_>>();

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&ExpectationsJson {
                source,
                total: filtered.len(),
                shown: shown.len(),
                search: args.search.clone(),
                status: status.map(|value| value.to_string()),
                expectations: shown,
            })
            .context("could not serialize expectations")?
        );
    } else {
        print_expectations_shortcut(
            &source,
            &filtered,
            &shown,
            args.search.as_deref(),
            status.as_ref(),
        );
    }
    Ok(())
}

pub(super) fn expectation_browse_rows(
    args: &ExpectationsArgs,
) -> Result<(String, Vec<ExpectationBrowseRow>)> {
    if let Some(file) = &args.file {
        let expectations = read_expectations_file(file)?;
        return Ok((
            file.display().to_string(),
            expectations
                .iter()
                .map(|expectation| expectation_browse_row(expectation, None))
                .collect(),
        ));
    }

    let paths = daily_review_paths(&args.target, Path::new(".susumu"));
    let work = paths.work.exists().then_some(paths.work);
    let analysis = load_analysis(&args.target, None, None, None, work.as_ref(), false)?;
    let support = expectation_support(&analysis)
        .into_iter()
        .map(|item| (item.expectation_id.clone(), item))
        .collect::<BTreeMap<_, _>>();
    Ok((
        args.target.display().to_string(),
        analysis
            .expectations
            .iter()
            .map(|expectation| expectation_browse_row(expectation, support.get(&expectation.id)))
            .collect(),
    ))
}

pub(super) fn expectation_browse_row(
    expectation: &Expectation,
    support: Option<&ExpectationSupport>,
) -> ExpectationBrowseRow {
    ExpectationBrowseRow {
        id: expectation.id.clone(),
        title: expectation.title.clone(),
        detail: expectation.detail.clone(),
        target: expectation.target.to_string(),
        subject: expectation.subject.clone(),
        status: expectation.status.to_string(),
        source: expectation.source.clone(),
        support_status: support.map(|support| support.support_status.clone()),
        target_observed: support.map(|support| support.target_observed),
        verification: support.map(|support| support.verification.clone()),
        work: support.map(|support| support.work),
        decisions: support.map(|support| support.decisions),
        findings: support.map(|support| support.findings),
    }
}

pub(super) fn filter_expectation_rows(
    mut rows: Vec<ExpectationBrowseRow>,
    search: Option<&str>,
    status: Option<&str>,
) -> Vec<ExpectationBrowseRow> {
    if let Some(status) = status {
        rows.retain(|row| row.status == status);
    }
    if let Some(search) = search.map(str::trim).filter(|search| !search.is_empty()) {
        let search = search.to_ascii_lowercase();
        rows.retain(|row| expectation_row_matches(row, &search));
    }
    rows.sort_by(|left, right| {
        left.status
            .cmp(&right.status)
            .then_with(|| left.id.cmp(&right.id))
    });
    rows
}

pub(super) fn expectation_row_matches(row: &ExpectationBrowseRow, search: &str) -> bool {
    [
        row.id.as_str(),
        row.title.as_str(),
        row.detail.as_str(),
        row.target.as_str(),
        row.subject.as_deref().unwrap_or("-"),
        row.status.as_str(),
        row.source.as_str(),
        row.support_status.as_deref().unwrap_or(""),
    ]
    .iter()
    .any(|value| value.to_ascii_lowercase().contains(search))
}

pub(super) fn print_expectations_shortcut(
    source: &str,
    filtered: &[ExpectationBrowseRow],
    shown: &[ExpectationBrowseRow],
    search: Option<&str>,
    status: Option<&ExpectationStatus>,
) {
    println!("Susumu expectations: {}", filtered.len());
    println!("Source: {source}");
    if let Some(search) = search.filter(|value| !value.trim().is_empty()) {
        println!("Search: {search}");
    }
    if let Some(status) = status {
        println!("Status: {status}");
    }
    if shown.is_empty() {
        println!("No matching expectations.");
        return;
    }

    let mut current_status = "";
    for row in shown {
        if row.status != current_status {
            current_status = &row.status;
            println!();
            println!("{}", expectation_status_heading(current_status));
        }
        println!("  {}", row.id);
        println!("    {}", row.title);
        println!(
            "    target={} subject={} source={}",
            row.target,
            row.subject.as_deref().unwrap_or("-"),
            row.source
        );
        if let Some(support_status) = &row.support_status {
            println!(
                "    support={} observed={} verification={}/{}/{} work={} decisions={} findings={}",
                support_status,
                row.target_observed.unwrap_or(false),
                row.verification
                    .as_ref()
                    .map_or(0, |verification| verification.passed),
                row.verification
                    .as_ref()
                    .map_or(0, |verification| verification.failed),
                row.verification
                    .as_ref()
                    .map_or(0, |verification| verification.inconclusive),
                row.work.unwrap_or(0),
                row.decisions.unwrap_or(0),
                row.findings.unwrap_or(0)
            );
        }
    }
    if filtered.len() > shown.len() {
        println!();
        println!("... {} more expectations", filtered.len() - shown.len());
    }
}

pub(super) fn expectation_status_heading(status: &str) -> &'static str {
    match status {
        "accepted" => "Accepted",
        "proposed" => "Proposed",
        "superseded" => "Superseded",
        _ => "Other",
    }
}

#[derive(Debug, Serialize)]
pub(super) struct VerifyJson {
    file: String,
    id: String,
    expectation: String,
    status: String,
    method: String,
    evidence: Option<String>,
    source: String,
}

pub(super) fn verify_shortcut(args: VerifyArgs) -> Result<()> {
    let status = verification_status_from_flags(&args)?;
    let analysis = load_analysis(&args.target, None, None, None, None, false)?;
    let expectation = analysis
        .expectations
        .iter()
        .find(|expectation| expectation.id == args.expectation)
        .with_context(|| {
            format!(
                "{} does not contain expectation {}; try `susumu expectations --search <term>`",
                args.target.display(),
                args.expectation
            )
        })?;
    let evidence = if let Some(path) = args.evidence_file.as_deref() {
        Some(hash_evidence_file(path)?)
    } else {
        args.evidence.filter(|value| !value.trim().is_empty())
    };
    let execution = args
        .execution_file
        .as_deref()
        .map(read_execution_file)
        .transpose()?;
    let detail = args.detail.unwrap_or_else(|| {
        format!(
            "Recorded by susumu verify. Expectation: {} - {}. Method: {}.",
            expectation.id, expectation.title, args.method
        )
    });
    let id = args.id.unwrap_or_else(|| {
        verification_id(
            &expectation.id,
            status,
            args.supersedes.as_deref(),
            &args.method,
            &args.source,
            evidence.as_deref(),
            &detail,
        )
    });
    let verification = Verification {
        id,
        expectation_id: expectation.id.clone(),
        status,
        supersedes: args.supersedes.filter(|value| !value.trim().is_empty()),
        execution,
        chain: None,
        method: args.method,
        source: args.source,
        evidence,
        basis: args.basis.filter(|value| !value.trim().is_empty()),
        detail,
    };
    let written = write_verification_record(&args.file, verification, args.minify)?;

    print_verification_result(&args.file, expectation, &written, args.json)?;

    Ok(())
}

pub(super) fn print_verification_result(
    file: &Path,
    expectation: &Expectation,
    written: &Verification,
    json: bool,
) -> Result<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&VerifyJson {
                file: file.display().to_string(),
                id: written.id.clone(),
                expectation: written.expectation_id.clone(),
                status: written.status.to_string(),
                method: written.method.clone(),
                evidence: written.evidence.clone(),
                source: written.source.clone(),
            })
            .context("could not serialize verification report")?
        );
    } else {
        println!("wrote verification {} to {}", written.id, file.display());
        println!("Expectation: {}  {}", expectation.id, expectation.title);
        println!("Status: {}", written.status);
        println!("Method: {}", written.method);
        println!("next:");
        println!("  susumu review");
    }
    Ok(())
}

pub(super) fn verification_status_from_flags(args: &VerifyArgs) -> Result<VerificationStatus> {
    match (args.passed, args.failed, args.inconclusive) {
        (true, false, false) => Ok(VerificationStatus::Passed),
        (false, true, false) => Ok(VerificationStatus::Failed),
        (false, false, true) => Ok(VerificationStatus::Inconclusive),
        (false, false, false) => {
            bail!("choose one verification status: --passed, --failed, or --inconclusive")
        }
        _ => bail!("choose only one verification status"),
    }
}

pub(super) fn write_verification_record(
    file: &Path,
    verification: Verification,
    minify: bool,
) -> Result<Verification> {
    let mut verifications = if file.exists() {
        read_verification_sidecar(&file.to_path_buf())?
    } else {
        Vec::new()
    };
    if let Some(existing) = verifications
        .iter()
        .find(|current| current.id == verification.id)
    {
        if existing == &verification {
            return Ok(existing.clone());
        }
        bail!(
            "verification {} already exists; use a new id and --supersedes to preserve history",
            verification.id
        );
    }
    if let Some(superseded_id) = verification.supersedes.as_deref() {
        let Some(superseded) = verifications
            .iter()
            .find(|current| current.id == superseded_id)
        else {
            bail!(
                "verification {} cannot supersede missing verification {}",
                verification.id,
                superseded_id
            );
        };
        if superseded.expectation_id != verification.expectation_id {
            bail!(
                "verification {} must supersede a record for expectation {}, not {}",
                verification.id,
                verification.expectation_id,
                superseded.expectation_id
            );
        }
    }
    merge_verifications(&mut verifications, vec![verification.clone()]);
    write_text_file(file, &write_verifications(&verifications, minify)?)?;
    Ok(verification)
}

pub(super) fn git_shortcut(args: &GitShortcutArgs) -> Result<()> {
    let artifact = git_shortcut_artifact(args)?;
    let connect_args = git_shortcut_connect_args(args);
    run_git_connect(&connect_args, &artifact)
}

pub(super) fn git_shortcut_artifact(args: &GitShortcutArgs) -> Result<ProjectAnalysis> {
    let work = args.output.exists().then_some(&args.output);
    load_analysis(&args.artifact, None, None, None, work, false)
}

pub(super) fn git_shortcut_connect_args(args: &GitShortcutArgs) -> GitConnectArgs {
    GitConnectArgs {
        repo: args.repo.clone(),
        artifact: args.artifact.clone(),
        since: args.since.clone(),
        until: args.until.clone(),
        limit: Some(args.limit),
        max_items: args.max_items,
        export_work: (!args.no_export).then_some(args.output.clone()),
        source: args.source.clone(),
        minify: args.minify,
        json: args.json,
    }
}

pub(super) fn daily_review_paths(target: &Path, output_dir: &Path) -> DailyReviewPaths {
    let base = conventional_output_dir(target, output_dir);
    DailyReviewPaths {
        artifact: base.join("project.susu"),
        packet: base.join("review.susu"),
        check_json: base.join("check.json"),
        html: base.join("review.html"),
        work: base.join("work.susu"),
    }
}

pub(super) fn conventional_output_dir(target: &Path, output_dir: &Path) -> PathBuf {
    if output_dir.is_absolute() {
        return output_dir.to_path_buf();
    }
    if target.is_dir() {
        target.join(output_dir)
    } else {
        output_dir.to_path_buf()
    }
}
