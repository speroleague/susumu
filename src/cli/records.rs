#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn add_expectation(args: AddExpectation) -> Result<()> {
    let target = ExpectationTarget::from(args.target);
    let status = ExpectationStatus::from(args.status);
    let subject = expectation_subject(
        target,
        resolve_file_subject(&args.target_root, target, args.subject)?,
    )?;
    let id = args.id.unwrap_or_else(|| {
        expectation_id(
            target,
            subject.as_deref(),
            status,
            &args.source,
            &args.title,
            &args.detail,
        )
    });
    let expectation = Expectation {
        id,
        target,
        subject,
        status,
        source: args.source,
        title: args.title,
        detail: args.detail,
    };

    let mut expectations = if args.file.exists() {
        read_expectation_sidecar(&args.file)?
    } else {
        Vec::new()
    };

    let id = expectation.id.clone();
    merge_expectations(&mut expectations, vec![expectation]);
    fs::write(&args.file, write_expectations(&expectations, false)?)
        .with_context(|| format!("could not write {}", args.file.display()))?;
    eprintln!("wrote expectation {id} to {}", args.file.display());
    Ok(())
}

pub(crate) fn resolve_target(args: &ResolveArgs) -> Result<()> {
    let analysis = scan_project(&args.target)
        .with_context(|| format!("could not scan {}", args.target.display()))?;
    let requested = normalize_git_path(&args.path.to_string_lossy());
    let matches = analysis
        .files
        .iter()
        .filter(|file| file.path == requested)
        .collect::<Vec<_>>();
    if matches.is_empty() {
        bail!(
            "could not resolve file path `{}` under {}; run `susumu resolve --help` for usage",
            args.path.display(),
            args.target.display()
        );
    }
    if args.json {
        println!(
            "{}",
            serde_json::json!({"path": matches[0].path, "id": matches[0].id})
        );
    } else {
        println!("{}\t{}", matches[0].id, matches[0].path);
    }
    Ok(())
}

pub(crate) fn resolve_file_subject(
    target_root: &Path,
    target: ExpectationTarget,
    subject: Option<String>,
) -> Result<Option<String>> {
    if target != ExpectationTarget::File {
        return Ok(subject);
    }
    let Some(subject) = subject else {
        return Ok(None);
    };
    if subject.starts_with("f_") {
        return Ok(Some(subject));
    }
    let analysis = scan_project(target_root).with_context(|| {
        format!(
            "could not scan {} while resolving file subject",
            target_root.display()
        )
    })?;
    let requested = normalize_git_path(&subject);
    let Some(file) = analysis.files.iter().find(|file| file.path == requested) else {
        bail!(
            "file subject `{subject}` did not resolve under {}; use `susumu resolve {subject}` or pass its f_ id",
            target_root.display()
        );
    };
    eprintln!("resolved file subject {subject} -> {}", file.id);
    Ok(Some(file.id.clone()))
}

pub(crate) fn list_expectations(args: &ListExpectations) -> Result<()> {
    let expectations = read_expectations_file(&args.file)?;
    if expectations.is_empty() {
        println!("No expectations in {}", args.file.display());
        return Ok(());
    }

    for expectation in expectations {
        let subject = expectation.subject.as_deref().unwrap_or("-");
        println!(
            "{}  {:9}  {:10}  {:18}  {}",
            expectation.id, expectation.target, expectation.status, subject, expectation.title
        );
    }
    Ok(())
}

pub(crate) fn remove_expectation(args: &RemoveExpectation) -> Result<()> {
    let mut expectations = read_expectation_sidecar(&args.file)?;
    let original_len = expectations.len();
    expectations.retain(|expectation| expectation.id != args.id);
    if expectations.len() == original_len {
        bail!(
            "{} does not contain expectation {}",
            args.file.display(),
            args.id
        );
    }

    fs::write(&args.file, write_expectations(&expectations, false)?)
        .with_context(|| format!("could not write {}", args.file.display()))?;
    eprintln!(
        "removed expectation {} from {}",
        args.id,
        args.file.display()
    );
    Ok(())
}

pub(crate) fn add_verification(args: AddVerification) -> Result<()> {
    let status = VerificationStatus::from(args.status);
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
    let id = args.id.unwrap_or_else(|| {
        verification_id(
            &args.expectation,
            status,
            args.supersedes.as_deref(),
            &args.method,
            &args.source,
            evidence.as_deref(),
            &args.detail,
        )
    });
    let verification = Verification {
        id,
        expectation_id: args.expectation,
        status,
        supersedes: args.supersedes.filter(|value| !value.trim().is_empty()),
        execution,
        chain: None,
        method: args.method,
        source: args.source,
        evidence,
        basis: args.basis.filter(|value| !value.trim().is_empty()),
        detail: args.detail,
    };

    let id = verification.id.clone();
    write_verification_record(&args.file, verification, false)?;
    eprintln!("wrote verification {id} to {}", args.file.display());
    Ok(())
}

pub(crate) fn list_verifications(args: &ListVerifications) -> Result<()> {
    let verifications = read_verifications_file(&args.file)?;
    if verifications.is_empty() {
        println!("No verifications in {}", args.file.display());
        return Ok(());
    }

    for verification in verifications {
        println!(
            "{}  {:12}  {:18}  {}",
            verification.id, verification.status, verification.expectation_id, verification.method
        );
    }
    Ok(())
}

pub(crate) fn remove_verification(args: &RemoveVerification) -> Result<()> {
    bail!(
        "verification records are append-only; cannot remove {} from {}. Add a new verification with --supersedes {} and the replacement status",
        args.id,
        args.file.display(),
        args.id
    )
}

#[derive(Debug, Serialize)]
pub(crate) struct VerificationChainReport {
    pub(crate) file: String,
    pub(crate) status: &'static str,
    pub(crate) records: usize,
    pub(crate) broken_at: Option<String>,
    pub(crate) tip: Option<String>,
    pub(crate) anchor_status: &'static str,
    pub(crate) note: &'static str,
}

pub(crate) fn verification_chain(args: &ChainVerificationArgs) -> Result<()> {
    let mut verifications = read_verification_sidecar(&args.file)?;
    if args.initialize {
        if verifications
            .iter()
            .any(|verification| verification.chain.is_some())
        {
            let report = verify_verification_chain(&args.file, &verifications);
            if report.status != "unchained" && report.status != "valid" {
                bail!(
                    "{} has a broken verification chain; repair it manually before reinitializing",
                    args.file.display()
                );
            }
            if report.status == "valid" {
                bail!("{} already has a verification chain", args.file.display());
            }
        }
        let mut previous = None;
        for verification in &mut verifications {
            let chain = verification_chain_digest(previous.as_deref(), verification);
            verification.chain = Some(chain.clone());
            previous = Some(chain);
        }
        fs::write(&args.file, write_verifications(&verifications, false)?)
            .with_context(|| format!("could not write {}", args.file.display()))?;
    }

    let report = verify_verification_chain(&args.file, &verifications);
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("could not serialize verification chain report")?
        );
    } else {
        println!("Verification chain: {}", report.status);
        println!("Records: {}", report.records);
        println!("Anchor: {}", report.anchor_status);
        if let Some(broken_at) = report.broken_at {
            println!("Broken at: {broken_at}");
        }
        println!("Note: {}", report.note);
    }
    Ok(())
}

pub(crate) fn verify_verification_chain(
    file: &Path,
    verifications: &[Verification],
) -> VerificationChainReport {
    if verifications.is_empty()
        || verifications
            .iter()
            .all(|verification| verification.chain.is_none())
    {
        return VerificationChainReport {
            file: file.display().to_string(),
            status: "unchained",
            records: verifications.len(),
            broken_at: None,
            tip: None,
            anchor_status: "none",
            note: "No chain is present. Initialize one for accidental-edit detection; a self-contained chain still needs an external trust anchor.",
        };
    }
    let mut previous = None;
    for verification in verifications {
        let expected = verification_chain_digest(previous.as_deref(), verification);
        if verification.chain.as_deref() != Some(expected.as_str()) {
            return VerificationChainReport {
                file: file.display().to_string(),
                status: "broken",
                records: verifications.len(),
                broken_at: Some(verification.id.clone()),
                tip: previous,
                anchor_status: "none",
                note: "The chain detects a changed, deleted, or reordered record, but it is not externally anchored.",
            };
        }
        previous.clone_from(&verification.chain);
    }
    VerificationChainReport {
        file: file.display().to_string(),
        status: "valid",
        records: verifications.len(),
        broken_at: None,
        tip: previous,
        anchor_status: "self_contained",
        note: "The chain is internally consistent and detects casual history changes; an external or signed tip is required to resist deliberate rewriting.",
    }
}

pub(crate) fn verification_chain_digest(
    previous: Option<&str>,
    verification: &Verification,
) -> String {
    let execution = verification
        .execution
        .as_ref()
        .map(|metadata| serde_json::to_string(metadata).unwrap_or_default())
        .unwrap_or_default();
    let mut hash = Sha256::new();
    for part in [
        previous.unwrap_or("-"),
        &verification.id,
        &verification.expectation_id,
        &verification.status.to_string(),
        verification.supersedes.as_deref().unwrap_or("-"),
        &verification.method,
        &verification.source,
        verification.evidence.as_deref().unwrap_or("-"),
        verification.basis.as_deref().unwrap_or("-"),
        &execution,
        &verification.detail,
    ] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    format!("sha256:{:x}", hash.finalize())
}

pub(crate) fn hash_evidence_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| {
        format!(
            "could not read verification evidence file {}",
            path.display()
        )
    })?;
    let mut hash = Sha256::new();
    hash.update(bytes);
    Ok(format!("sha256:{:x}", hash.finalize()))
}

pub(crate) fn read_execution_file(path: &Path) -> Result<VerificationExecution> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("could not read execution metadata file {}", path.display()))?;
    let execution = serde_json::from_str(&source)
        .with_context(|| format!("could not parse execution metadata file {}", path.display()))?;
    Ok(execution)
}

pub(crate) fn inspect_attestation(args: &InspectAttestationArgs) -> Result<()> {
    let inspection = attestation::inspect_file(&args.file)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&inspection)
                .context("could not serialize attestation inspection")?
        );
    } else {
        println!("Attestation: {}", inspection.attestation_id);
        println!("Posture: {}", inspection.posture);
        println!("Trust: {}", inspection.trust_status);
        println!("Note: {}", inspection.note);
    }
    Ok(())
}

pub(crate) fn inspect_git_signature(args: &GitSignatureArgs) -> Result<()> {
    let inspection = git_signature::inspect(&args.repo, &args.commit)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&inspection)
                .context("could not serialize Git signature inspection")?
        );
    } else {
        println!("Commit: {}", inspection.commit);
        println!("Signature: {}", inspection.status);
        println!(
            "Signer: {}",
            inspection.signer.as_deref().unwrap_or("unknown")
        );
        println!(
            "Fingerprint: {}",
            inspection.fingerprint.as_deref().unwrap_or("unknown")
        );
        println!("Execution: {}", inspection.execution_status);
        println!("Note: {}", inspection.note);
    }
    Ok(())
}

pub(crate) fn add_decision(args: AddDecision) -> Result<()> {
    let target = ExpectationTarget::from(args.target);
    let status = DecisionStatus::from(args.status);
    let subject = target_subject("decisions", target, args.subject)?;
    let id = args.id.unwrap_or_else(|| {
        decision_id(
            target,
            subject.as_deref(),
            status,
            &args.source,
            &args.title,
            &args.detail,
        )
    });
    let decision = Decision {
        id,
        target,
        subject,
        status,
        source: args.source,
        basis: args.basis.filter(|value| !value.trim().is_empty()),
        title: args.title,
        detail: args.detail,
    };

    let mut decisions = if args.file.exists() {
        read_decision_sidecar(&args.file)?
    } else {
        Vec::new()
    };

    let id = decision.id.clone();
    merge_decisions(&mut decisions, vec![decision]);
    fs::write(&args.file, write_decisions(&decisions, false)?)
        .with_context(|| format!("could not write {}", args.file.display()))?;
    eprintln!("wrote decision {id} to {}", args.file.display());
    Ok(())
}

pub(crate) fn list_decisions(args: &ListDecisions) -> Result<()> {
    let decisions = read_decisions_file(&args.file)?;
    if decisions.is_empty() {
        println!("No decisions in {}", args.file.display());
        return Ok(());
    }

    for decision in decisions {
        let subject = decision.subject.as_deref().unwrap_or("-");
        println!(
            "{}  {:9}  {:10}  {:18}  {}",
            decision.id, decision.target, decision.status, subject, decision.title
        );
    }
    Ok(())
}

pub(crate) fn remove_decision(args: &RemoveDecision) -> Result<()> {
    let mut decisions = read_decision_sidecar(&args.file)?;
    let original_len = decisions.len();
    decisions.retain(|decision| decision.id != args.id);
    if decisions.len() == original_len {
        bail!(
            "{} does not contain decision {}",
            args.file.display(),
            args.id
        );
    }

    fs::write(&args.file, write_decisions(&decisions, false)?)
        .with_context(|| format!("could not write {}", args.file.display()))?;
    eprintln!("removed decision {} from {}", args.id, args.file.display());
    Ok(())
}

pub(crate) fn add_work(args: AddWork) -> Result<()> {
    let target = ExpectationTarget::from(args.target);
    let subject = target_subject("work records", target, args.subject)?;
    let expectation = args.expectation.filter(|value| !value.trim().is_empty());
    let kind = WorkKind::from(args.kind);
    let status = WorkStatus::from(args.status);
    let evidence = args.evidence.filter(|value| !value.trim().is_empty());
    let id = args.id.unwrap_or_else(|| {
        work_id(
            target,
            subject.as_deref(),
            expectation.as_deref(),
            kind,
            status,
            &args.source,
            evidence.as_deref(),
            &args.title,
            &args.detail,
        )
    });
    let work = Work {
        id,
        target,
        subject,
        expectation_id: expectation,
        kind,
        status,
        source: args.source,
        evidence,
        title: args.title,
        detail: args.detail,
    };

    let mut works = if args.file.exists() {
        read_work_sidecar(&args.file)?
    } else {
        Vec::new()
    };

    let id = work.id.clone();
    merge_works(&mut works, vec![work]);
    fs::write(&args.file, write_works(&works, false)?)
        .with_context(|| format!("could not write {}", args.file.display()))?;
    eprintln!("wrote work {id} to {}", args.file.display());
    Ok(())
}

pub(crate) fn list_works(args: &ListWorks) -> Result<()> {
    let works = read_works_file(&args.file)?;
    if works.is_empty() {
        println!("No work records in {}", args.file.display());
        return Ok(());
    }

    for work in works {
        let subject = work.subject.as_deref().unwrap_or("-");
        let expectation = work.expectation_id.as_deref().unwrap_or("-");
        println!(
            "{}  {:9}  {:14}  {:11}  {:18}  {:18}  {}",
            work.id, work.target, work.kind, work.status, subject, expectation, work.title
        );
    }
    Ok(())
}

pub(crate) fn remove_work(args: &RemoveWork) -> Result<()> {
    let mut works = read_work_sidecar(&args.file)?;
    let original_len = works.len();
    works.retain(|work| work.id != args.id);
    if works.len() == original_len {
        bail!("{} does not contain work {}", args.file.display(), args.id);
    }

    fs::write(&args.file, write_works(&works, false)?)
        .with_context(|| format!("could not write {}", args.file.display()))?;
    eprintln!("removed work {} from {}", args.id, args.file.display());
    Ok(())
}
