use anyhow::{Context, Result};
use serde::Serialize;
use std::{path::Path, process::Command};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct GitSignatureInspection {
    pub repository: String,
    pub commit: String,
    pub status: &'static str,
    pub signer: Option<String>,
    pub fingerprint: Option<String>,
    pub raw_status: String,
    pub execution_status: &'static str,
    pub note: &'static str,
}

pub(crate) fn inspect(repo: &Path, commit: &str) -> Result<GitSignatureInspection> {
    let output = Command::new("git")
        .args(["verify-commit", "--raw", commit])
        .current_dir(repo)
        .output()
        .with_context(|| format!("could not run git verify-commit in {}", repo.display()))?;
    let raw_status = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .trim()
    .to_owned();
    let (status, signer, fingerprint) = classify(&raw_status, output.status.success());
    Ok(GitSignatureInspection {
        repository: repo.display().to_string(),
        commit: commit.to_owned(),
        status,
        signer,
        fingerprint,
        raw_status,
        execution_status: "not_checked",
        note: "Git signature status covers commit identity/integrity only; it does not prove that tests ran or that any compliance requirement was met.",
    })
}

fn classify(raw: &str, success: bool) -> (&'static str, Option<String>, Option<String>) {
    let mut signer = None;
    let mut fingerprint = None;
    for line in raw.lines() {
        if let Some(value) = line.strip_prefix("[GNUPG:] GOODSIG ") {
            signer = value
                .split_once(' ')
                .map_or_else(|| Some(value.to_owned()), |(_, name)| Some(name.to_owned()));
        }
        if let Some(value) = line.strip_prefix("[GNUPG:] VALIDSIG ") {
            fingerprint = value.split_whitespace().next().map(str::to_owned);
        }
    }
    let status = if success {
        "verified_identity_integrity"
    } else if raw.contains("BADSIG") || raw.contains("ERRSIG") || raw.contains("NO_PUBKEY") {
        "signature_present_unverified"
    } else if raw.is_empty() || raw.to_ascii_lowercase().contains("no signature") {
        "unsigned"
    } else {
        "verification_failed"
    };
    (status, signer, fingerprint)
}

#[cfg(test)]
mod tests {
    use super::classify;

    #[test]
    fn parses_valid_signature_identity_without_execution_claim() {
        let (status, signer, fingerprint) = classify(
            "[GNUPG:] GOODSIG ABC123 Jane Reviewer\n[GNUPG:] VALIDSIG FINGERPRINT 2026-07-20 0 4 0 1 10 ABC123",
            true,
        );
        assert_eq!(status, "verified_identity_integrity");
        assert_eq!(signer.as_deref(), Some("Jane Reviewer"));
        assert_eq!(fingerprint.as_deref(), Some("FINGERPRINT"));
    }

    #[test]
    fn distinguishes_bad_signature_from_unsigned_commit() {
        assert_eq!(
            classify("[GNUPG:] BADSIG ABC123", false).0,
            "signature_present_unverified"
        );
        assert_eq!(classify("error: no signature", false).0, "unsigned");
        assert_eq!(classify("", false).0, "unsigned");
    }
}
