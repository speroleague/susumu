# Susumu feature inventory

This is the baseline inventory for the repository-connected collaboration work. New API and frontend features must preserve these behaviors, records, output surfaces, and trust boundaries.

## Product surfaces

- Rust CLI for scanning, authoring records, Git history, review packets, readiness, checks, diffs, handoffs, attestation inspection, and daily workflows.
- Ratatui TUI for engineering review, evidence browsing, source context, connections, and threaded review ownership.
- Portable `.susu` artifacts and sidecars for project evidence, expectations, verifications, decisions, work, and review threads.
- Standalone HTML portal for stakeholder review. The exported HTML is read-only and must not imply that it can write project records.
- Machine-readable JSON for checks, readiness, handoffs, packet summaries, Git operations, diffs, and attestation inspection.
- CI and GitHub Pages workflows that build, retain, and publish review artifacts.

These surfaces are complementary rather than role-locked:

| Surface | Business and operations | Engineering | CI and automation |
| --- | --- | --- | --- |
| Live API and frontend | Review, search, discuss, assign, and follow repository evidence | Same collaboration workflows plus engineering evidence and source context | Consume or update through authenticated service credentials where configured |
| Static HTML export | Read-only review and evidence browsing | Read-only review and handoff context | Published artifact and regression surface |
| TUI | Optional | Primary interactive engineering workbench | Not normally used |
| CLI and JSON | Optional scripting and exports | Primary authoring, inspection, Git, and review commands | Primary automation, checks, packet builds, and readiness gates |

## Observed evidence

The scanner currently records:

- project identity, root, schema, and generation time;
- supported source files, language, line count, byte count, and content hash when available;
- symbols with stable ids, kinds, locations, and source-region fingerprints;
- dependencies and source locations;
- framework-level HTTP workflows with method, path, handler, entry symbol, file, location, and resolution confidence;
- symbol-to-symbol call flows, including unresolved external or ambiguous edges;
- syntax-highlighted source previews in review packets when source is readable;
- deterministic workflow attention scores with inspectable reasons.

The current adapter boundary covers Rust, PHP, Python, JavaScript, TypeScript, TSX, and Vue-family source handling. The scanner skips unsupported, unreadable, or over-limit files with visible findings rather than inventing evidence.

## Authored records

All authored records remain distinct from scanner observations:

- `expectation`: business intent, policy, requirement, acceptance criterion, or authored project expectation;
- `verification`: a reported check result with method, status, source, optional evidence, execution metadata, and basis;
- `decision`: authored judgment, approval, rejection, exception, or unresolved choice with optional basis;
- `work`: activity claimed by a person, agent, import, or automation, optionally linked to an expectation and evidence;
- `review`: threaded discussion with target, parent, lifecycle status, owner, source, title, and detail.

Expectation, verification, decision, work, and review sidecars are mergeable, portable, inspectable, and safe to edit without rewriting scanner evidence. Human-authored source labels are provenance declarations, not authenticated identity.

## Derived analysis and findings

Current deterministic findings include:

- `SUS000`/`SUS006`: unreadable, skipped, or unsupported source observations;
- `SUS001`: large source file;
- `SUS002`: long workflow unit;
- `SUS003`: high fan-out;
- `SUS004`: ambiguous call targets;
- `SUS005`: recursive or cyclic call flow;
- `SUS010`-`SUS012`: malformed or stale expectation targets;
- `SUS020`: verification points at a missing expectation;
- `SUS023`: verification basis changed and needs renewed review;
- `SUS030`-`SUS033`: malformed, stale, or changed decision targets;
- `SUS040`-`SUS043`: malformed, stale, or missing work targets and expectation links;
- `SUS050`-`SUS054`: malformed, stale, orphaned, or cyclic review-thread links.

Findings are derived signals. They do not silently alter authored status, certify a control, or turn a discussion into verification.

## Review and readiness behavior

The review system currently provides:

- review queues for failed or inconclusive verifications, stale verification and decision bases, missing links, scanner findings, unresolved workflow gaps, open review threads, and work needing verification;
- portable review anchors for expectations, verifications, work, decisions, findings, and source locations, with typed contributions, owners, parent replies, and missing-anchor findings;
- expectation support summaries with target observation, linked work, verification posture, decision context, findings, and next action;
- readiness buckets for failed verification, missing target, work needing verification, no linked work, verified, and unknown;
- human-readable and JSON readiness output with search and bucket filters;
- review packets containing the artifact, check report, handoff state, readiness state, support summaries, next actions, and source previews;
- packet diffing and Git rewind comparison with stale-evidence reporting;
- TUI review and connections jumps to the relevant record or review-thread context;
- portal overview, readiness, review, threads, workflow evidence, traceability, source, records, dirty/stale evidence, artifact, and next-action views.

## Git and history

The Git surface currently supports:

- import of changed commits into work records;
- commit correlation by changed files, Susumu ids, expectation targets, record targets, and existing evidence links;
- explicit `git link` work records for expectations;
- suggestions and JSON output for unconnected commits;
- signature inspection for configured GPG/SSH commit verification;
- safe snapshot reconstruction and rewind comparison without mutating the working tree;
- source-revision provenance plus exact or candidate migration reports during `git rewind`;
- stale-review detection for changed source, expectations, linked work, verification bases, and decision bases.

Git signatures authenticate commit identity and integrity only. They do not authenticate test execution, business approval, or compliance conclusions.

## Compliance and verification posture

Susumu supports an organization’s compliance process by organizing evidence, provenance, review, and retention context. It does not certify compliance or decide that a control or regulation has been met.

The current posture levels are:

- declared;
- content-bound;
- externally authenticated, only when a configured verifier accepts an attestation;
- human-reviewed, under the organization’s own process.

Attestation inspection is structural unless a future configured verifier authenticates the issuer, signature, execution claims, artifact retention, and policy. Execution metadata remains declared until authenticated. Hashes establish byte identity, not test execution or compliance. Review and decision bases produce renewed-review findings when their supporting evidence changes.

Any collaboration backend must preserve these distinctions. A comment, owner assignment, approval, objection, pull request, merge, or authenticated user action must not be rendered as a verification result or compliance certification.

## Current limitations to preserve visibly

- The live API authenticates application users, stores append-only audit events, and keeps synchronization state server-side. Portable review records still carry declared human provenance; an authenticated session is not proof that a claim is true.
- The live API now provides authenticated, project and branch-scoped ranked fuzzy search over indexed record summaries, with filters, pagination, synchronization upserts, and periodic base-branch refresh.
- The static portal is read-only; the optional authenticated frontend is the mutation surface for
  repository registration, connection setup, structured records, review comments, and synchronization.
- The CLI’s `source` field is declared metadata, not identity authentication.
- Full resource-shaped thread actions, webhook processing, richer server-derived timeline resources,
  and authenticated user-management screens remain future frontend/API slices. The current portal
  creates portable review records through the repository synchronization path.
- Source ids are stable for ordinary scans. Artifacts record the Git revision when available, and `git rewind` reports exact or candidate file, symbol, and workflow migrations across renamed or refactored source without silently rewriting authored records.
- Dirty propagation is present for important target and basis changes but needs broader, more precise historical coverage.
- The seven current language families demonstrate the adapter boundary; additional adapters are not the near-term product priority.
- AI assistance is not required by the core scanner or review workflow and must remain optional, labeled, cited, and human-reviewable.

## Contract for the next phase

The API, CLI, TUI, CI integrations, static exports, and live frontend should consume the same packet and record model. The live backend may add authenticated actor, timestamp, audit-event, repository-connection, and synchronization metadata, but it must export portable `.susu` records without weakening the evidence vocabulary above.

The GitHub integration is a repository connection managed by an administrator, not a required identity provider for business users. Each connected repository may have one active Susumu synchronization branch and pull request; synchronization state is never global to the Susumu deployment. A configured base branch selects the repository’s current lifecycle context. New changes update that repository pull request until it is merged; the next change after merge begins a new synchronization cycle for that repository.
