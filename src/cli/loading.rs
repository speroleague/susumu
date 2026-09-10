#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) fn load_analysis(
    target: &PathBuf,
    expectations: Option<&PathBuf>,
    verifications: Option<&PathBuf>,
    decisions: Option<&PathBuf>,
    work: Option<&PathBuf>,
    reviews: Option<&PathBuf>,
    log_merges: bool,
) -> Result<ProjectAnalysis> {
    let (mut analysis, is_artifact) = load_base_analysis(target)?;
    load_sidecars(
        &mut analysis,
        &SidecarInputs {
            target,
            expectations,
            verifications,
            decisions,
            work,
            reviews,
            is_artifact,
            log_merges,
        },
    )?;
    finalize_loaded_analysis(&mut analysis);

    Ok(analysis)
}

/// Best-effort load of a project analysis for stamping evidence provenance
/// (basis + revision) at record time. Uses the same convention sidecars the
/// daily review loop loads. Returns `None` when the path is not a project.
pub(crate) fn load_for_stamp(project: &Path) -> Option<ProjectAnalysis> {
    if !project.is_dir() {
        return None;
    }
    let project = project.to_path_buf();
    let work = daily::daily_work_sidecar(&project);
    load_analysis(&project, None, None, None, work.as_ref(), None, false).ok()
}

/// Records the review basis and source revision Susumu observed when a
/// verification was written, unless the caller supplied them explicitly.
pub(crate) fn stamp_verification(verification: &mut Verification, analysis: &ProjectAnalysis) {
    if verification.basis.is_none() {
        verification.basis = current_basis_for_verification(analysis, verification);
    }
    if verification.revision.is_none() {
        verification.revision.clone_from(&analysis.source_revision);
    }
}

/// Records the review basis and source revision Susumu observed when a
/// decision was written, unless the caller supplied them explicitly.
pub(crate) fn stamp_decision(decision: &mut Decision, analysis: &ProjectAnalysis) {
    if decision.basis.is_none() {
        decision.basis = current_basis_for_decision(analysis, decision);
    }
    if decision.revision.is_none() {
        decision.revision.clone_from(&analysis.source_revision);
    }
}

struct SidecarInputs<'a> {
    target: &'a Path,
    expectations: Option<&'a PathBuf>,
    verifications: Option<&'a PathBuf>,
    decisions: Option<&'a PathBuf>,
    work: Option<&'a PathBuf>,
    reviews: Option<&'a PathBuf>,
    is_artifact: bool,
    log_merges: bool,
}

fn load_sidecars(analysis: &mut ProjectAnalysis, inputs: &SidecarInputs<'_>) -> Result<()> {
    refresh_derived_analysis(analysis);
    let expectation_path = sidecar_path(
        inputs.target,
        inputs.expectations,
        inputs.is_artifact,
        "expectations.susu",
    );
    let verification_path = sidecar_path(
        inputs.target,
        inputs.verifications,
        inputs.is_artifact,
        "verifications.susu",
    );
    let review_path = sidecar_path(
        inputs.target,
        inputs.reviews,
        inputs.is_artifact,
        "reviews.susu",
    );
    merge_expectation_sidecar(analysis, expectation_path.as_deref(), inputs.log_merges)?;
    merge_verification_sidecar(analysis, verification_path.as_deref(), inputs.log_merges)?;
    merge_decision_sidecar(analysis, inputs.decisions, inputs.log_merges)?;
    merge_work_sidecar(analysis, inputs.work, inputs.log_merges)?;
    merge_review_thread_sidecar(analysis, review_path.as_deref(), inputs.log_merges)
}

fn finalize_loaded_analysis(analysis: &mut ProjectAnalysis) {
    anchor_verification_bases(analysis);
    anchor_decision_bases(analysis);
    refresh_derived_analysis(analysis);
}

fn load_base_analysis(target: &PathBuf) -> Result<(ProjectAnalysis, bool)> {
    let is_artifact = target
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("susu"));
    let analysis = if is_artifact {
        let source = fs::read_to_string(target)
            .with_context(|| format!("could not read {}", target.display()))?;
        parse_susu(&source).with_context(|| format!("could not parse {}", target.display()))?
    } else {
        if !target.is_dir() {
            bail!("{} is not a directory or .susu file", target.display());
        }
        scan_project(target)?
    };
    Ok((analysis, is_artifact))
}

fn sidecar_path(
    target: &Path,
    explicit: Option<&PathBuf>,
    is_artifact: bool,
    filename: &str,
) -> Option<PathBuf> {
    explicit.cloned().or_else(|| {
        (!is_artifact)
            .then(|| target.join(filename))
            .filter(|candidate| candidate.exists())
    })
}

fn merge_expectation_sidecar(
    analysis: &mut ProjectAnalysis,
    path: Option<&Path>,
    log_merges: bool,
) -> Result<()> {
    let Some(path) = path else { return Ok(()) };
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    let imported = parse_expectations(&source)
        .with_context(|| format!("could not parse expectations from {}", path.display()))?;
    let count = imported.len();
    merge_expectations(&mut analysis.expectations, imported);
    refresh_derived_analysis(analysis);
    if log_merges {
        eprintln!("merged {count} expectations from {}", path.display());
    }
    Ok(())
}

fn merge_verification_sidecar(
    analysis: &mut ProjectAnalysis,
    path: Option<&Path>,
    log_merges: bool,
) -> Result<()> {
    let Some(path) = path else { return Ok(()) };
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    let imported = parse_verifications(&source)
        .with_context(|| format!("could not parse verifications from {}", path.display()))?;
    let count = imported.len();
    merge_verifications(&mut analysis.verifications, imported);
    refresh_derived_analysis(analysis);
    if log_merges {
        eprintln!("merged {count} verifications from {}", path.display());
    }
    Ok(())
}

fn merge_decision_sidecar(
    analysis: &mut ProjectAnalysis,
    path: Option<&PathBuf>,
    log_merges: bool,
) -> Result<()> {
    let Some(path) = path else { return Ok(()) };
    let source = fs::read_to_string(path)
        .with_context(|| format!("could not read decisions from {}", path.display()))?;
    let imported = parse_decisions(&source)
        .with_context(|| format!("could not parse decisions from {}", path.display()))?;
    let count = imported.len();
    merge_decisions(&mut analysis.decisions, imported);
    refresh_derived_analysis(analysis);
    if log_merges {
        eprintln!("merged {count} decisions from {}", path.display());
    }
    Ok(())
}

fn merge_work_sidecar(
    analysis: &mut ProjectAnalysis,
    path: Option<&PathBuf>,
    log_merges: bool,
) -> Result<()> {
    let Some(path) = path else { return Ok(()) };
    let source = fs::read_to_string(path)
        .with_context(|| format!("could not read work from {}", path.display()))?;
    let imported = parse_works(&source)
        .with_context(|| format!("could not parse work from {}", path.display()))?;
    let count = imported.len();
    merge_works(&mut analysis.works, imported);
    refresh_derived_analysis(analysis);
    if log_merges {
        eprintln!("merged {count} work records from {}", path.display());
    }
    Ok(())
}

fn merge_review_thread_sidecar(
    analysis: &mut ProjectAnalysis,
    path: Option<&Path>,
    log_merges: bool,
) -> Result<()> {
    let Some(path) = path else { return Ok(()) };
    let source = fs::read_to_string(path)
        .with_context(|| format!("could not read reviews from {}", path.display()))?;
    let imported = parse_review_threads(&source)
        .with_context(|| format!("could not parse reviews from {}", path.display()))?;
    let count = imported.len();
    merge_review_threads(&mut analysis.review_threads, imported);
    if log_merges {
        eprintln!("merged {count} review threads from {}", path.display());
    }
    Ok(())
}
