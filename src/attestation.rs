use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct AttestationEnvelope {
    pub schema_version: String,
    pub attestation_id: String,
    pub expectation_id: Option<String>,
    pub verification_id: Option<String>,
    pub artifact_digests: Vec<String>,
    pub execution: Option<ExecutionClaim>,
    pub issuer: Option<String>,
    pub workflow_identity: Option<String>,
    pub issued_at: Option<String>,
    pub run_id: Option<String>,
    pub signature: Option<SignatureClaim>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct ExecutionClaim {
    pub result: String,
    pub command: Option<String>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct SignatureClaim {
    pub algorithm: String,
    pub key_id: Option<String>,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AttestationInspection {
    pub posture: &'static str,
    pub schema_version: String,
    pub attestation_id: String,
    pub expectation_id: Option<String>,
    pub verification_id: Option<String>,
    pub artifact_digests: Vec<String>,
    pub has_execution_claim: bool,
    pub has_signature_claim: bool,
    pub trust_status: &'static str,
    pub note: &'static str,
}

pub(crate) fn inspect_file(path: &Path) -> Result<AttestationInspection> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("could not read attestation file {}", path.display()))?;
    let envelope: AttestationEnvelope = serde_json::from_str(&source)
        .with_context(|| format!("could not parse attestation file {}", path.display()))?;
    validate(&envelope)?;
    Ok(AttestationInspection {
        posture: "declared",
        schema_version: envelope.schema_version,
        attestation_id: envelope.attestation_id,
        expectation_id: envelope.expectation_id,
        verification_id: envelope.verification_id,
        artifact_digests: envelope.artifact_digests,
        has_execution_claim: envelope.execution.is_some(),
        has_signature_claim: envelope.signature.is_some(),
        trust_status: "not_verified",
        note: "Structural inspection only; no signature, issuer, execution, retention, or compliance claim was authenticated.",
    })
}

fn validate(envelope: &AttestationEnvelope) -> Result<()> {
    if envelope.schema_version.trim().is_empty() {
        bail!("attestation schema_version must not be empty");
    }
    if envelope.attestation_id.trim().is_empty() {
        bail!("attestation attestation_id must not be empty");
    }
    if envelope.expectation_id.is_none() && envelope.verification_id.is_none() {
        bail!("attestation must identify an expectation_id or verification_id");
    }
    if envelope
        .artifact_digests
        .iter()
        .any(|digest| digest.trim().is_empty())
    {
        bail!("attestation artifact_digests must not contain empty values");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AttestationEnvelope, ExecutionClaim, SignatureClaim, inspect_file};
    use std::fs;

    #[test]
    fn inspects_attestation_as_declared_without_trusting_signature() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("attestation.json");
        let envelope = AttestationEnvelope {
            schema_version: "susumu.attestation/v1".to_owned(),
            attestation_id: "att_1".to_owned(),
            expectation_id: Some("e_1".to_owned()),
            verification_id: None,
            artifact_digests: vec!["sha256:abc".to_owned()],
            execution: Some(ExecutionClaim {
                result: "passed".to_owned(),
                command: Some("cargo test".to_owned()),
                exit_code: Some(0),
            }),
            issuer: Some("runner.example".to_owned()),
            workflow_identity: Some("workflow-1".to_owned()),
            issued_at: Some("2026-07-20T00:00:00Z".to_owned()),
            run_id: Some("run-1".to_owned()),
            signature: Some(SignatureClaim {
                algorithm: "example-signature".to_owned(),
                key_id: Some("key-1".to_owned()),
                value: "not-verified-here".to_owned(),
            }),
        };
        fs::write(
            &file,
            serde_json::to_vec(&envelope).expect("serialize envelope"),
        )
        .expect("write envelope");

        let inspection = inspect_file(&file).expect("inspect envelope");
        assert_eq!(inspection.posture, "declared");
        assert_eq!(inspection.trust_status, "not_verified");
        assert!(inspection.has_execution_claim);
        assert!(inspection.has_signature_claim);
    }
}
