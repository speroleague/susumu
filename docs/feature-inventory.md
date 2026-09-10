# Susumu feature inventory

This is the baseline inventory of Susumu's current behaviors, records, output surfaces, and trust boundaries. New features must preserve them.

## Product surfaces

- Rust CLI for scanning, authoring records, Git history, review packets, readiness, digests, checks, diffs, handoffs, attestation inspection, and daily workflows.
- Ratatui TUI for engineering review, evidence browsing, source context, connections, and threaded review ownership.
- Portable `.susu` artifacts and sidecars for project evidence, expectations, verifications, decisions, work, and review threads.
- Standalone HTML portal for stakeholder review. The exported HTML is read-only and must not imply that it can write project records.
- Machine-readable JSON for checks (with a changed-evidence count), readiness, digests, handoffs, packet summaries, Git operations, diffs, and attestation inspection.
- CI and GitHub Pages workflows that build, retain, and publish review artifacts.

These surfaces are complementary rather than role-locked:

| Surface | Business and operations | Engineering | CI and automation |
| --- | --- | --- | --- |
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
- `SUS023`: verification basis changed and needs renewed review; commit-attributed when the record carries a `revision`;
- `SUS056`: an authored record still references a source identity from an older revision and needs explicit migration review;
- `SUS030`-`SUS033`: malformed, stale, or changed decision targets;
- `SUS040`-`SUS043`: malformed, stale, or missing work targets and expectation links;
- `SUS050`-`SUS054`: malformed, stale, orphaned, or cyclic review-thread links.

Findings are derived signals. They do not silently alter authored status, certify a control, or turn a discussion into verification.

## Review and readiness behavior

The review system currently provides:

- review queues for failed or inconclusive verifications, stale verification and decision bases, missing links, scanner findings, unresolved workflow gaps, open review threads, and work needing verification;
- portable review anchors for expectations, verifications, work, decisions, findings, and source locations, with typed contributions, owners, parent replies, and missing-anchor findings;
- expectation support summaries with target observation, linked work, verification posture, decision context, findings, and next action;
- readiness buckets for failed verification, missing target, changed evidence needing re-verification, work needing verification, no linked work, verified, and unknown;
- a `susumu digest` command that rescans and prints only the actionable items — re-verification needs with the causing commit, checks, open review threads, and unverified expectations — in text or JSON;
- human-readable and JSON readiness output with search and bucket filters;
- review packets containing the artifact, check report, handoff state, readiness state, support summaries, next actions, and source previews;
- packet diffing and Git rewind comparison with stale-evidence reporting;
- commit-attributed dirty findings: `susumu review` and `susumu check` walk the Git history since a verification or decision's recorded `revision` and name the commit(s) that changed the target, with a `susumu check --fail-on-dirty` gate;
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

Any review surface must preserve these distinctions. A comment, owner assignment, approval, or objection must not be rendered as a verification result or compliance certification.

## Current limitations to preserve visibly

- The static HTML portal is read-only. Records are authored and changed only with the CLI or TUI.
- Portable review records carry declared human provenance; a `source` label is not proof that a claim is true.
- The CLI’s `source` field is declared metadata, not identity authentication.
- Source ids are stable for ordinary scans. Artifacts record the Git revision when available, and `git rewind` reports exact or candidate file, symbol, and workflow migrations across renamed or refactored source without silently rewriting authored records.
- Dirty propagation includes target, expectation, linked work, review-thread context, and source-migration changes, while unresolved migration findings remain explicit and require authored repair.
- The seven current language families demonstrate the adapter boundary; additional adapters are not the near-term product priority.
- AI assistance is not required by the core scanner or review workflow and must remain optional, labeled, cited, and human-reviewable.

## Contract for new work

The CLI, TUI, CI integrations, and static HTML export all consume the same packet and record model. Any new surface must read and write portable `.susu` records without weakening the evidence vocabulary above.
