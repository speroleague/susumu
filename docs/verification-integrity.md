# Verification integrity and provenance

Susumu can help a development team and business assemble evidence for an organization's own review and compliance process. Susumu does not certify compliance, decide that a control or regulation has been met, or promise that it will be met. A person or organization remains responsible for the conclusion.

## Provenance levels

These levels describe the evidence available to a reviewer. They are not compliance ratings and must not be rendered as certification badges.

| Level | What Susumu may know | What it does not prove |
| --- | --- | --- |
| Declared | Someone entered a source label, method, or evidence reference. | That the named person, runner, or process actually produced it. |
| Content-bound | A referenced artifact was available to `susumu verify --evidence-file` and its bytes were hashed. | That the artifact came from CI, that a check ran, or that the artifact remains available. |
| Externally authenticated | A future verifier accepted a signed or otherwise authenticated attestation from a configured trust source. | That the underlying control is satisfied, that the attestation is complete, or that an auditor must accept it. |
| Human-reviewed | An authorized reviewer evaluated the evidence under the organization's process. | That the review is legally sufficient or that another organization will reach the same conclusion. |

The current `source` field is declared metadata. Strings such as `ci:runner-a` are not authenticated merely because they contain `ci:`. Susumu must never place a declared record in an “attested” bucket.

## Provider-neutral attestation contract

CI/CD integrations should produce a portable attestation input rather than a vendor-specific Susumu record. A future verifier may accept a JSON or equivalent signed envelope containing:

- a schema version and unique attestation id;
- the expectation or verification id being supported;
- the artifact digest(s), using an explicit algorithm such as `sha256`;
- the execution result and the exact command or job input, if available;
- the producing workflow identity and issuer;
- issuance time, validity or expiration information, and a replay-resistant run id;
- a signature or other verifiable proof tied to a configured trust policy.

The envelope should reference artifacts by digest and stable external identifier. It should not require Susumu to upload source code, test output, secrets, or personal data. Retention and access to the referenced artifact remain the responsibility of the CI/CD and organizational systems that own it.

Verification records may also carry execution metadata supplied through `--execution-file`. This makes result, exit code, run id, issuance time, and an artifact-manifest reference portable, but those values are declared until a configured verifier authenticates them.

Provider adapters may translate GitHub, GitLab, Jenkins, Buildkite, a private runner, or another system into this contract, but the core Susumu model must not assume any one provider. An adapter that only copies a URL or accepts a user-supplied `ci:*` string is a convenience integration, not authentication.

Susumu can inspect a commit with `git signature`. This reuses Git's configured GPG/SSH signature verification and records signer/integrity posture without adding a Susumu-specific key infrastructure. A valid signature authenticates the commit, not the execution of tests or the satisfaction of a control. A repository owner must still decide which keys, identities, and branch protections are trusted.

## Review language

Use language such as “declared,” “content hash recorded,” “attestation accepted under policy X,” or “human review recorded.” Avoid “certified,” “compliant,” “control met,” “audit approved,” and similar conclusions in Susumu-generated output. The next action should identify the missing organizational review, retention, provenance, or trust decision rather than silently promoting a record to compliance.
