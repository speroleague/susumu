use crate::*;

pub(crate) fn normalize_git_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_owned()
}

pub(crate) fn expectation_subject(
    target: ExpectationTarget,
    subject: Option<String>,
) -> Result<Option<String>> {
    target_subject("expectations", target, subject)
}

pub(crate) fn target_subject(
    noun: &str,
    target: ExpectationTarget,
    subject: Option<String>,
) -> Result<Option<String>> {
    match (target, subject) {
        (ExpectationTarget::Project, None) => Ok(None),
        (ExpectationTarget::Project, Some(_)) => {
            bail!("project {noun} are project-wide; omit --subject")
        }
        (_, Some(subject)) => Ok(Some(subject)),
        (_, None) => bail!("{target} {noun} require --subject"),
    }
}

pub(crate) fn expectation_id(
    target: ExpectationTarget,
    subject: Option<&str>,
    status: ExpectationStatus,
    source: &str,
    title: &str,
    detail: &str,
) -> String {
    let mut hash = Sha256::new();
    for part in [
        target.to_string(),
        subject.unwrap_or("-").to_owned(),
        status.to_string(),
        source.to_owned(),
        title.to_owned(),
        detail.to_owned(),
    ] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    format!("e_{}", hex_prefix(&hash.finalize(), 8))
}

pub(crate) fn verification_id(
    expectation_id: &str,
    status: VerificationStatus,
    supersedes: Option<&str>,
    method: &str,
    source: &str,
    evidence: Option<&str>,
    detail: &str,
) -> String {
    let mut hash = Sha256::new();
    for part in [
        expectation_id.to_owned(),
        status.to_string(),
        supersedes.unwrap_or("-").to_owned(),
        method.to_owned(),
        source.to_owned(),
        evidence.unwrap_or("-").to_owned(),
        detail.to_owned(),
    ] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    format!("v_{}", hex_prefix(&hash.finalize(), 8))
}

pub(crate) fn decision_id(
    target: ExpectationTarget,
    subject: Option<&str>,
    status: DecisionStatus,
    source: &str,
    title: &str,
    detail: &str,
) -> String {
    let mut hash = Sha256::new();
    for part in [
        target.to_string(),
        subject.unwrap_or("-").to_owned(),
        status.to_string(),
        source.to_owned(),
        title.to_owned(),
        detail.to_owned(),
    ] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    format!("d_{}", hex_prefix(&hash.finalize(), 8))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn work_id(
    target: ExpectationTarget,
    subject: Option<&str>,
    expectation: Option<&str>,
    kind: WorkKind,
    status: WorkStatus,
    source: &str,
    evidence: Option<&str>,
    title: &str,
    detail: &str,
) -> String {
    let mut hash = Sha256::new();
    for part in [
        target.to_string(),
        subject.unwrap_or("-").to_owned(),
        expectation.unwrap_or("-").to_owned(),
        kind.to_string(),
        status.to_string(),
        source.to_owned(),
        evidence.unwrap_or("-").to_owned(),
        title.to_owned(),
        detail.to_owned(),
    ] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    format!("w_{}", hex_prefix(&hash.finalize(), 8))
}

pub(crate) fn hex_prefix(bytes: &[u8], count: usize) -> String {
    let mut output = String::with_capacity(count * 2);
    for byte in bytes.iter().take(count) {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

pub(crate) fn has_records_other_than(source: &str, allowed_kind: &str) -> bool {
    let mut statement_start = 0;
    let mut in_string = false;
    let mut escaped = false;

    for (index, character) in source.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }

        if character == '"' {
            in_string = true;
        } else if character == ';' {
            if statement_has_record_other_than(&source[statement_start..index], allowed_kind) {
                return true;
            }
            statement_start = index + character.len_utf8();
        }
    }

    statement_has_record_other_than(&source[statement_start..], allowed_kind)
}

pub(crate) fn statement_has_record_other_than(statement: &str, allowed_kind: &str) -> bool {
    statement
        .split_whitespace()
        .next()
        .is_some_and(|kind| kind != allowed_kind)
}

pub(crate) fn read_expectations_file(path: &PathBuf) -> Result<Vec<Expectation>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    parse_expectations(&source)
        .with_context(|| format!("could not parse expectations from {}", path.display()))
}

pub(crate) fn read_expectation_sidecar(path: &PathBuf) -> Result<Vec<Expectation>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    if has_records_other_than(&source, "expectation") {
        bail!(
            "{} looks like a full .susu artifact; use an expectation-only sidecar file",
            path.display()
        );
    }
    parse_expectations(&source)
        .with_context(|| format!("could not parse expectations from {}", path.display()))
}

pub(crate) fn read_verifications_file(path: &PathBuf) -> Result<Vec<Verification>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    parse_verifications(&source)
        .with_context(|| format!("could not parse verifications from {}", path.display()))
}

pub(crate) fn read_verification_sidecar(path: &PathBuf) -> Result<Vec<Verification>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    if has_records_other_than(&source, "verification") {
        bail!(
            "{} looks like a full .susu artifact; use a verification-only sidecar file",
            path.display()
        );
    }
    parse_verifications(&source)
        .with_context(|| format!("could not parse verifications from {}", path.display()))
}

pub(crate) fn read_decisions_file(path: &PathBuf) -> Result<Vec<Decision>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    parse_decisions(&source)
        .with_context(|| format!("could not parse decisions from {}", path.display()))
}

pub(crate) fn read_decision_sidecar(path: &PathBuf) -> Result<Vec<Decision>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    if has_records_other_than(&source, "decision") {
        bail!(
            "{} looks like a full .susu artifact; use a decision-only sidecar file",
            path.display()
        );
    }
    parse_decisions(&source)
        .with_context(|| format!("could not parse decisions from {}", path.display()))
}

pub(crate) fn read_works_file(path: &PathBuf) -> Result<Vec<Work>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    parse_works(&source).with_context(|| format!("could not parse work from {}", path.display()))
}

pub(crate) fn read_work_sidecar(path: &PathBuf) -> Result<Vec<Work>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    if has_records_other_than(&source, "work") {
        bail!(
            "{} looks like a full .susu artifact; use a work-only sidecar file",
            path.display()
        );
    }
    parse_works(&source).with_context(|| format!("could not parse work from {}", path.display()))
}

pub(crate) fn merge_expectations(existing: &mut Vec<Expectation>, imported: Vec<Expectation>) {
    for expectation in imported {
        existing.retain(|current| current.id != expectation.id);
        existing.push(expectation);
    }
}

pub(crate) fn merge_verifications(existing: &mut Vec<Verification>, imported: Vec<Verification>) {
    for verification in imported {
        existing.retain(|current| current.id != verification.id);
        existing.push(verification);
    }
}

pub(crate) fn merge_decisions(existing: &mut Vec<Decision>, imported: Vec<Decision>) {
    for decision in imported {
        existing.retain(|current| current.id != decision.id);
        existing.push(decision);
    }
}

pub(crate) fn merge_works(existing: &mut Vec<Work>, imported: Vec<Work>) {
    for work in imported {
        existing.retain(|current| current.id != work.id);
        existing.push(work);
    }
}
