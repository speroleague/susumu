# Susumu

Susumu makes an existing software project explainable.

It sits between source control, specifications, reviews, and business decisions. Point it at a repository and it builds a deterministic evidence model of source files, symbols, dependencies, workflows, call flows, ambiguity, expectations, verifications, decisions, and work records.

Susumu is local-first today: a Rust scanner, terminal workbench, review packet generator, Git connector, and standalone web review surface. The core scan, check, review packet, and portal loop runs without AI keys. The project is growing toward shared project memory for engineers, AI agents, business stakeholders, and reviewers: what the system is, what it does, what is expected of it, what work addressed those expectations, and what people have decided or questioned.

The optional Dockerized Rust API and Caddy frontend are documented in [Server deployment](docs/deployment.md). They provide authenticated repository collaboration, PostgreSQL-backed users and audit state, multiple encrypted GitHub App connections, API-backed search, structured record authoring, and one-active-PR synchronization per repository. The local CLI, TUI, static HTML export, live portal, API, and CI remain clients of the same project-memory model.

[View the live Susumu review portal.](https://speroleague.github.io/susumu/)

## Start here

Susumu is for teams that need to connect business intent, source code, implementation work, checks,
decisions, and review conversation without creating a second version of the project. It does not
replace Git, your test runner, or your project-management system. It adds a portable evidence and
review layer around them.

Choose the path that fits your role:

| If you are... | Start with... | You will get... |
| --- | --- | --- |
| An engineer or agent | The local CLI and TUI | Fast scans, exact source locations, Git links, readiness, and JSON for automation |
| A business or operations reviewer | The live Docker portal | A branded workspace for repositories, records, search, ownership, threads, and structured authoring |
| A team sharing review output | The static HTML export | A read-only, portable review packet suitable for GitHub Pages or internal hosting |
| A CI or platform owner | The CLI, JSON, and Docker deployment guides | Repeatable checks, review artifacts, authenticated API workflows, and per-repository synchronization |

The local CLI and static portal do not need AI keys or a backend. The live portal is optional and
adds authenticated users, PostgreSQL state, GitHub App connections, API search, and pull-request
synchronization. All surfaces consume the same `.susu` project-memory model.

## The daily loop

Rust 1.88 or newer is required.

Start a project:

```sh
cargo run -- init ./path/to/project --name "My Project"
```

Use Susumu day to day:

```sh
cargo run -- review
cargo run -- status
cargo run -- readiness
cargo run -- git --since main
cargo run -- expectations --search git
cargo run -- verify e_susumu_easy_daily_cli --passed --method "cargo test --locked"
cargo run -- review-thread add --target project --owner team-platform --title "Clarify deployment ownership" --detail "Who owns the deployment decision?"
cargo run -- review
cargo run -- open
```

That is the core workflow:

1. `review` scans the project and writes the current review files.
2. `status` shows the review queue without opening the portal.
3. `readiness` shows expectation readiness counts and next actions from the latest review packet.
4. `git --since main` connects recent commits to expectations and writes work records.
5. `expectations --search git` helps you find the right expectation id when a commit needs an explicit link.
6. `verify` records how an expectation was checked.
7. `review` runs again so the new work and verification records appear in the packet.
8. `review-thread` records authored discussion, replies, and ownership without turning them into verification or approval.
9. `open` starts the local stakeholder/engineering portal.

For business and operations users, the live portal adds repositories through an administrator setup flow. An administrator can reuse an existing GitHub App connection or add another one, choose an available repository, select one of its branches, and begin working without editing `.susu` text. A repository conflict pauses new authoring until the guided resolver is completed.

By convention:

- Humans author `expectations.susu`.
- Humans or automation record checks in `verifications.susu`.
- Susumu writes `.susumu/project.susu`.
- Susumu writes `.susumu/review.susu`.
- Susumu writes `.susumu/check.json`.
- Susumu writes `.susumu/review.html`.
- Susumu writes `.susumu/work.susu` when Git work is exported.

Keep authored intent in normal source control. Keep generated `.susumu/` files local unless you intentionally publish or attach them.

Brand the generated portal with `susumu.toml` in the project root:

```toml
[portal]
title = "My Project Memory"
background = "#11131a"
panel = "#1a1f2b"
text = "#e8e2d7"
muted = "#aaa292"
line = "#363b49"
accent = "#9eb7a0"
```

`review`, `review --serve`, `open`, and `review export-html` load this file automatically when it is present. The config only changes the portal shell; `.susu` review packets remain portable evidence.

For the philosophy and team workflow, read [The Susumu Way](docs/the-susumu-way.md).

## Documentation map

- [The Susumu Way](docs/the-susumu-way.md) - the normal daily workflow and how evidence becomes reviewable.
- [Server deployment](docs/deployment.md) - local Docker testing and production deployment responsibilities.
- [Feature inventory](docs/feature-inventory.md) - the current capability and limitation checklist.
- [Artifact contract](docs/artifact.md) - the portable `.susu` record model and evidence boundaries.
- [Collaboration backend](docs/collaboration-backend.md) - users, repositories, GitHub connections, synchronization, threads, and conflicts.
- [Verification integrity and provenance](docs/verification-integrity.md) - what evidence levels mean and what they do not prove.
- [Language and framework adapters](docs/adapters.md) - supported scanner ecosystems and extension boundaries.
- [Product architecture](docs/vision.md) - the long-term product model and delivery sequence.
- [Susumu vernacular](docs/vernacular.md) - language rules for scanner, human, verification, and AI-generated statements.

## Where Susumu is today

Susumu is pre-1.0, but it is already an end-to-end product rather than only a scanner. The local
path is stable enough to use for project review and CI: scan source, author expectations, connect
Git work, record verifications, inspect readiness, and publish a portable HTML review. The optional
live path adds authenticated collaboration around connected repositories.

| Available now | Still being expanded |
| --- | --- |
| Deterministic scanning for seven language families and common HTTP workflow patterns | More workflow types such as jobs, queues, events, tests, databases, and deployments |
| Portable `.susu` artifacts for observations, expectations, verifications, decisions, work, and review threads | Source-revision migration across renames and refactors |
| CLI, TUI, static HTML, JSON, and CI review surfaces | Broader and more precise historical dirty-state propagation |
| Docker API and Caddy portal with local users, PostgreSQL, multiple GitHub App connections, API search, structured forms, anchored threads, ownership, and timelines | Richer server resource endpoints, webhooks, permissions management, and release snapshots |
| One active synchronization PR per connected repository, with base advancement and guided conflict resolution | Optional BYOK AI assistance after the deterministic review model is stronger |

Business users do not need GitHub accounts. A Susumu administrator connects one or more GitHub Apps
to the deployment, then users work through Susumu's authenticated portal. GitHub remains the source
control provider and pull-request review boundary; Susumu does not write directly to a base branch.
See the [feature inventory](docs/feature-inventory.md) for the detailed current contract and
limitations.

## Live repository portal

The optional Docker deployment provides the authenticated workspace for business, operations, and engineering users. It can be deployed with Docker Engine and Compose on Linux, macOS, or Windows:

```sh
cp .env.example .env
docker compose up --build
```

On PowerShell, use `Copy-Item .env.example .env` for the first command. Open `http://localhost:3000` after the containers are healthy. To stop the local stack, run `docker compose down`.

An administrator can add a named GitHub App connection, reuse an existing connection, select an available repository, and choose one of its branches. The setup flow stores encrypted credentials in PostgreSQL, not in `.env`; set `SUSUMU_CREDENTIAL_KEY` once in the deployment environment.

The live portal can inspect repository records, search through the API, show record detail and anchored threads, author structured expectations, verifications, work, and review comments, and submit bounded changes through the repository's active PR. The standalone `review.html` export remains read-only.

## Common commands

Create starter expectations:

```sh
cargo run -- init ./path/to/project --name "Checkout Service"
```

Build the daily review files:

```sh
cargo run -- review
```

Build and immediately serve the portal:

```sh
cargo run -- review --serve
```

Open the latest static portal export in your default browser:

```sh
cargo run -- open
```

Use the local server when the portal needs to be served over HTTP:

```sh
cargo run -- open --serve
```

Print the saved review summary instead of serving:

```sh
cargo run -- open --summary
```

Check current status:

```sh
cargo run -- status
```

Show expectation readiness from the latest packet:

```sh
cargo run -- readiness
cargo run -- readiness --bucket needs_verification
cargo run -- readiness --search git
cargo run -- readiness --json
```

Browse or search expectation ids:

```sh
cargo run -- expectations
cargo run -- expectations --search git
cargo run -- expectations --status accepted
```

Record verification evidence:

```sh
cargo run -- verify e_susumu_easy_daily_cli --passed --method "cargo test --locked"
cargo run -- verify e_susumu_docs_teach_daily_workflow --failed --method "manual docs review"
cargo run -- verify e_susumu_git_work_support --inconclusive --method "reviewed local git output"
```

Connect Git work to the latest Susumu artifact:

```sh
cargo run -- git --since main
```

Explicitly link an ambiguous commit to an expectation:

```sh
cargo run -- git link abc123 e_susumu_docs_teach_daily_workflow --kind documentation
```

When `git` reports an unconnected commit, it prints likely expectations and copyable `susumu git link ...` commands when it has enough language overlap to suggest candidates.

Run the engineering TUI on the latest generated artifact:

```sh
cargo run -- .\.susumu\project.susu
```

## How the records fit together

Susumu keeps different kinds of truth separate:

- Scanner evidence says what was observed in code.
- Expectations say what humans or a business expect.
- Verifications say how an expectation was checked.
- Decisions say what judgment was made.
- Work records say what humans, agents, imports, or automation changed or reviewed.

The scanner can determine support, not satisfaction. If a commit links to an expectation, Susumu can show that work supports the expectation. It still needs verification before the expectation should be treated as proven.

## Current capabilities

- Scans Rust, PHP, Python, JavaScript, TypeScript, TSX, and Vue single-file components through language-specific adapters. Vue script blocks are parsed with the TypeScript/TSX Tree-sitter grammar while template and style blocks remain outside scanner evidence.
- Keeps per-language parser behavior behind `src/language/adapters.rs`, so new ecosystems can be added without changing `.susu` consumers.
- Respects `.gitignore`, `.ignore`, Git excludes, hidden-directory filtering, and a 2 MiB per-file safety limit.
- Extracts modules, functions, methods, imports, and calls using Tree-sitter.
- Detects initial HTTP workflows for Express-compatible, FastAPI-compatible, Flask-compatible, Laravel, Symfony, Axum-compatible, and Actix Web conventions.
- Ranks workflows with deterministic attention scores and explicit reasons.
- Uses deterministic path/name-based ids so evidence and review targets survive ordinary rescans.
- Resolves same-file calls exactly and unique project-wide calls as likely.
- Preserves ambiguous and external calls as visible gaps.
- Reports deterministic findings for large files, long workflow units, high fan-out, ambiguous targets, parse recovery, and call cycles.
- Carries authored expectations, verification records, decision records, work records, and threaded review records with explicit owners.
- Shows threaded review records and ownership in both the standalone portal and the engineering TUI.
- Provides authenticated live portal review with repository switching, searchable record lists, record detail pages, anchored threads, replies, ownership, timelines, and structured forms for expectations, verifications, work, and review comments.
- Supports multiple encrypted GitHub App connections. Each connected repository records which App connection it uses, and repository discovery and branch selection are scoped to that connection.
- Maintains one active synchronization branch and pull request per repository, not for the system as a whole. New changes update that repository's active PR until it is merged, then the next change starts a new lifecycle.
- Detects base-branch advancement, preserves conflicts, and provides a guided record-level resolver. Unique records are included automatically; matching record IDs require an explicit choice.
- Refreshes the API search index during inspection, after successful sidecar synchronization, and periodically so externally merged changes become searchable.
- Flags review threads whose target or reply parent is missing, so discussion remains traceable to current evidence.
- Summarizes expectation support and machine-readable readiness queues in review packets.
- Shows an expectation evidence ladder in the portal: observed target, linked work, verification evidence, decision context, review status, and the next suggested action.
- Groups expectations by readiness in the portal and surfaces dirty or stale evidence with nearby syntax-highlighted source when Susumu can locate it.
- Connects local Git commits to workflows, expectations, and exported work records, including separate work records when one commit supports multiple expectations.
- Builds a Review queue from stale decisions, failed or inconclusive verification records, scanner findings, and unresolved workflow gaps.
- Explores overview metrics, review items, expectations, verifications, decisions, work records, detected workflows, call flows, findings, and files in a Ratatui interface.
- Exports a standalone web portal with workflow drill-down and syntax-highlighted source previews.
- Reads and writes the versioned `.susu` syntax in readable or minified form.

This first model is a call-flow model, not yet full variable-level data lineage. That distinction matters: Susumu should grow its evidence carefully rather than overstate what static analysis can prove.

## Demo project

The demo project is shaped like a small product system: TypeScript checkout routes, PHP Laravel-style routes, Rust Axum-style routes, authored expectations, verification records, decision records, and work records.

Build a rich demo artifact:

```sh
cargo run -- .\examples\demo-project --expectations .\examples\demo-project\expectations.susu --verifications .\examples\demo-project\verifications.susu --decisions .\examples\demo-project\decisions.susu --work .\examples\demo-project\work.susu --output .\target\susumu-demo.susu --headless
```

Open it in the TUI:

```sh
cargo run -- .\target\susumu-demo.susu
```

Create and serve a demo review portal:

```sh
cargo run -- review build .\examples\demo-project --expectations .\examples\demo-project\expectations.susu --verifications .\examples\demo-project\verifications.susu --decisions .\examples\demo-project\decisions.susu --work .\examples\demo-project\work.susu --artifact-output .\target\susumu-demo.susu --output .\target\susumu-demo.review.susu --html .\target\susumu-demo.html
cargo run -- review serve .\target\susumu-demo.review.susu
```

## Advanced command reference

The short commands are the normal path. The commands below expose the underlying plumbing when you need explicit files, CI behavior, fixtures, or advanced review comparisons.

Scan a repository, write an artifact, and skip the TUI:

```sh
cargo run -- ./path/to/project --output project.susu --headless
```

Merge authored sidecars while scanning:

```sh
cargo run -- ./path/to/project --expectations expectations.susu --verifications verifications.susu --decisions decisions.susu --work work.susu --output project.susu --headless
```

Create or update expectation sidecars:

```sh
cargo run -- expectation add --file expectations.susu --target project --source human:product --title "Keep architecture explainable" --detail "The project should keep scanner evidence, authored intent, and review feedback in the same portable artifact."
cargo run -- resolve src/main.rs
cargo run -- expectation add --file expectations.susu --target file --subject src/main.rs --target-root . --title "Keep the CLI explainable" --detail "The CLI should keep its review workflow understandable."
cargo run -- expectation list --file expectations.susu
cargo run -- expectation remove --file expectations.susu e_91bbd1
```

Create or list verification sidecars:

```sh
cargo run -- verification add --file verifications.susu --expectation e_91bbd1 --status passed --method "cargo test checkout_order" --source ci:github-actions --evidence run:123456 --detail "The checkout order test passed in CI."
cargo run -- verification add --file verifications.susu --expectation e_91bbd1 --status passed --method "test runner invocation" --evidence-file target/test-report.xml --detail "The locally retained report is content-addressed without copying its contents into the sidecar."
cargo run -- verification add --file verifications.susu --expectation e_91bbd1 --status passed --method "test runner invocation" --evidence-file target/test-report.xml --execution-file target/execution.json --detail "Execution metadata is recorded as supplied by the runner and remains untrusted until separately authenticated."
cargo run -- verification list --file verifications.susu
cargo run -- verification add --file verifications.susu --expectation e_91bbd1 --status inconclusive --supersedes v_checkout_order --method "Await retained CI evidence" --source human:engineer --detail "The prior verification is no longer relied on for this review."
cargo run -- verification chain --file verifications.susu --initialize
cargo run -- verification chain --file verifications.susu --json
```

Verification sidecars are append-only through supported commands. `verification remove` fails so a prior assertion cannot be silently removed from the supported workflow; use a new record with `--supersedes` to document a replacement or retraction. This is workflow integrity, not tamper-proofing: ordinary text files remain subject to Git and filesystem controls until a future anchored integrity feature is used.

`--evidence-file` records only a `sha256:<digest>` of a local artifact; Susumu does not upload, retain, or inspect the artifact contents. This is provider-neutral and works with any CI/CD system that can make an evidence file available to the command. A content hash proves only that the referenced bytes match later; it does not prove that a test ran, who produced the file, or that any compliance requirement was met. Retention, provenance, execution claims, and human review remain separate responsibilities.

`--execution-file` accepts JSON with `result`, optional `exit_code`, `run_id`, `issued_at`, and `artifact_manifest`. Susumu records these as declared execution metadata; it does not authenticate the producer, timestamp, command, run, manifest, or compliance result.

`verification chain --initialize` adds a SHA-256 chain over the verification records. `verification chain` reports `valid`, `broken`, or `unchained`. The chain detects casual edits, deletions, and reordering, but a self-contained chain is not an external trust anchor: someone who can rewrite the entire file can recompute it. Anchor the tip in a protected or signed system before relying on it against deliberate rewriting.

Susumu can support an organization's compliance process by organizing evidence, review state, provenance, and retention information. It does not certify compliance, determine that a control or regulation has been met, or promise that it will be met. Any compliance conclusion remains the responsibility of the relevant people, process, and organization.

The planned provenance model and provider-neutral attestation boundary are documented in [Verification integrity and provenance](docs/verification-integrity.md).

Inspect a provider-neutral attestation envelope:

```sh
cargo run -- attestation inspect --file .\target\attestation.json --json
```

Inspection validates the envelope shape and reports `posture=declared` with `trust_status=not_verified`. It does not verify a signature, issuer, execution claim, artifact retention, or compliance outcome.

Create, list, or remove decision sidecars:

```sh
cargo run -- decision add --file decisions.susu --target workflow --subject w_8feec23b6a19d218 --status accepted --source human:director --title "Accept checkout exception" --detail "The team accepts this implementation exception for the current release with follow-up verification required."
cargo run -- decision list --file decisions.susu
cargo run -- decision remove --file decisions.susu d_checkout_exception
```

Create, list, or remove work sidecars:

```sh
cargo run -- work add --file work.susu --target workflow --subject w_8feec23b6a19d218 --expectation e_91bbd1 --kind implementation --status completed --source agent:codex --evidence commit:abc123 --title "Update checkout reservation" --detail "Updated checkout so inventory reservation happens before payment capture."
cargo run -- work list --file work.susu
cargo run -- work remove --file work.susu wk_checkout_agent
```

Create, list, or remove threaded review records:

```sh
cargo run -- review-thread add --file reviews.susu --target project --owner team-platform --title "Clarify deployment ownership" --detail "Who owns the deployment decision?"
cargo run -- review-thread add --file reviews.susu --target project --parent r_abc123 --status resolved --owner team-platform --title "Ownership assigned" --detail "Platform owns the follow-up."
cargo run -- review-thread list --file reviews.susu
cargo run -- review-thread list --file reviews.susu --status open --owner team-platform
cargo run -- review-thread remove --file reviews.susu r_abc123
```

The daily review build automatically loads `reviews.susu` from an initialized project. Review records may target a project, file, symbol, or workflow, and may carry an explicit anchor such as `expectation:e_123`, `verification:v_123`, `work:w_123`, or `source:src/main.rs#42`; `--parent` creates a reply and `--kind` records a question, objection, approval, risk, clarification, decision request, or comment. `--status` and `--owner` filter the CLI list for focused follow-up. Open threads appear as warning-level review work with an owner and next action. Ownership is an authored responsibility label, not an access-control or approval claim. The standalone portal exposes an owner workload summary, live text search, owner/status filters, and next actions in its Threads view, keeping discussion separate from verification evidence. The live portal shows anchored threads when a user opens a record and can submit replies or new threads through the configured review service. The exported HTML is read-only; replies and lifecycle changes require a separately configured review service.

Import local Git commits as work records:

```sh
cargo run -- git import --since main --output work.susu
cargo run -- git import --since main --artifact project.susu --target-depth workflow --output work.susu --json
```

`git import` creates one completed work record per commit. Without `--artifact`, imports stay project-wide. With `--artifact`, `--target-depth file` targets exactly one matched artifact file, while `--target-depth workflow` targets exactly one workflow from the changed files. Ambiguous commits stay file- or project-level instead of guessing.

Connect Git commits to Susumu workflows and records:

```sh
cargo run -- git connect --artifact project.susu --since main
cargo run -- git connect --artifact project.susu --limit 25 --json
cargo run -- git connect --artifact project.susu --since main --export-work work.susu
```

`git connect` is read-only unless `--export-work` is supplied. It correlates commits with the current artifact by changed workflow files, explicit Susumu ids in commit messages, expectation targets, verification/decision targets, and existing work records with `evidence="commit:<sha>"`.

Inspect Git commit signature identity and integrity:

```sh
cargo run -- git signature --repo . --commit HEAD --json
```

This delegates to Git's configured signature verification and reports signer/fingerprint status when available. A verified signature authenticates the commit identity and integrity only; it does not prove that tests ran, that an artifact was retained, or that a compliance requirement was met. The command does not make GitHub, GPG, or SSH a required Susumu provider.

For unconnected commits, `git connect` also prints a `next:` section. If it can infer likely expectation candidates, it includes ready-to-copy `susumu git link <commit> <expectation-id>` commands. If it cannot infer candidates, it still shows the generic link command and suggests listing expectations.

When a commit is valid work but Susumu refuses to guess which expectation it supports, link it explicitly:

```sh
cargo run -- expectations --search docs
cargo run -- git link abc123 e_susumu_docs_teach_daily_workflow
cargo run -- git link abc123 e_susumu_docs_teach_daily_workflow --kind documentation --detail "Documentation now teaches the daily workflow first."
```

`git link` reads `.susumu/project.susu` by default, validates the expectation id, resolves the commit, and writes or updates `.susumu/work.susu`. It does not rewrite Git history.

Compare the current artifact against code evidence from an older Git ref:

```sh
cargo run -- git rewind --from HEAD~1 --fail-on-stale
cargo run -- git rewind --from main --json --fail-on-stale
cargo run -- git rewind --from main --artifact .susumu/project.susu --json
cargo run -- git rewind --from main --artifact project.susu --old-output old-main.susu
```

`git rewind` reconstructs the selected ref into a temporary snapshot without checking out or mutating the repository, scans that snapshot, scans the current repository when `--artifact` is omitted, and runs the same comparison model as `diff`. It also reports exact or candidate migrations for files, symbols, and workflows whose source identity moved between revisions, plus `SUS056` findings when authored records still point at the old identity. `--fail-on-stale` makes it suitable for CI or a pre-merge gate.

Review a migration before changing authored records:

```sh
cargo run -- migrate old-main.susu project.susu
cargo run -- migrate old-main.susu project.susu --accept old-file-id=new-file-id --output migrated.susu
cargo run -- migrate old-main.susu project.susu --reject old-symbol-id --defer another-old-id --json
```

`migrate` is preview-first. `--accept OLD_ID=NEW_ID` is required for every retargeting, and the
replacement must be one of the candidates Susumu reported. Accepted mappings can also update
explicit sidecars with `--expectations`, `--decisions`, `--work`, and `--reviews`; pass `--work`
to append the human disposition as an auditable review work record. `--reject` and `--defer`
record the reviewer disposition in JSON or the optional work audit without rewriting source
evidence. The TUI's Findings view shows `SUS056` items and their migration guidance when an
artifact contains unresolved migration findings.

Check an artifact or project for review blockers:

```sh
cargo run -- check project.susu
cargo run -- check ./path/to/project --expectations expectations.susu --verifications verifications.susu --decisions decisions.susu --work work.susu
cargo run -- check project.susu --json
```

`check` exits nonzero when critical review items are present, such as failed verifications. Add `--strict` to also fail on warnings such as stale targets, inconclusive verifications, blocked work, or changed verification/decision evidence.

Record verification from the daily workflow:

```sh
cargo run -- verify e_susumu_easy_daily_cli --passed --method "cargo test --locked"
```

`verify` validates the expectation against the current project, writes or updates `verifications.susu`, and prints `susumu review` as the next step. The easy `review` path automatically loads both `expectations.susu` and `verifications.susu` from initialized repositories. Use the advanced `verification add` command when you need explicit ids, sidecar-only workflows, or lower-level scripting.

Create a handoff brief for a human reviewer or the next agent:

```sh
cargo run -- handoff project.susu
cargo run -- handoff project.susu --json
```

Create point-in-time review packets:

```sh
cargo run -- review build ./path/to/project --artifact-output project.susu --output review.susu --check-json check.json --html review.html
cargo run -- review create project.susu --output review.susu
cargo run -- review create project.susu --json
```

Open, compare, serve, or export review packets:

```sh
cargo run -- review open review.susu
cargo run -- review open review.susu --tui
cargo run -- review diff old.review.susu new.review.susu
cargo run -- review diff old.review.susu new.review.susu --json
cargo run -- review serve review.susu
cargo run -- review export-html review.susu --output review.html
```

Compare two artifacts:

```sh
cargo run -- diff old.susu new.susu
cargo run -- diff old.susu new.susu --fail-on-stale
cargo run -- diff old.susu new.susu --json
```

Write the compact form:

```sh
cargo run -- ./path/to/project --output project.min.susu --minify --headless
```

## TUI controls

Inside the TUI, use `1` through `9`, `0`, or `Tab` to change views, `j`/`k` or the arrow keys to move, `Enter` to jump from a Review or Connections item to its source record, `b` to go back, `e` to export readable syntax, `m` to export minified syntax, and `q` to quit. The `0` shortcut jumps to the final Files tab; use `Tab` or the arrow keys for any tab between `9` and `0`, including Review threads.

## CI and pull requests

This repository includes a GitHub Actions workflow at `.github/workflows/ci.yml`.

The `Rust checks` job runs formatting, tests, and Clippy as hard gates. The `Susumu self-review packet` job builds the repository's own `.susu` artifact, automatically loading `expectations.susu` and `verifications.susu`, writes machine-readable `check.json`, creates a `review.susu` packet, exports the standalone `review.html` portal, verifies those files exist, and uploads them as a retained workflow artifact named `susumu-review-<run-id>`.

On pushes to `main`, the `Publish Susumu portal` job also publishes the latest generated portal to GitHub Pages. The deployed site uses the same artifact bundle as the CI review packet: `index.html` is copied from `review.html`, and `project.susu`, `check.json`, and `review.susu` remain available beside it for humans, agents, and automation. Open-source projects can use this as an always-current public project memory page; companies can use the same pattern with an internal static host.

Uploaded PR artifacts include:

- `project.susu` - the current deterministic project evidence model.
- `check.json` - machine-readable review/check output.
- `review.susu` - the portable review packet for humans and agents.
- `review.html` - the standalone stakeholder review portal.

The self-review job records review findings in `check.json` but does not fail just because the packet contains warnings, because those findings are useful output for the review artifact. In a production repository, use `cargo run -- status --strict`, `cargo run -- check project.susu --strict`, or `cargo run -- diff old.susu new.susu --fail-on-stale` when the review signal should block a pull request.

To enable the Pages deployment in GitHub, configure the repository's Pages source to use GitHub Actions. Pull requests still receive retained review artifacts without deploying a public site.

## A `.susu` artifact

```susu
susu version=1;
project name="checkout" root="C:\\code\\checkout" generated=1784052000;
file f_6c481624d8960b19 path="src/main.rs" language=rust lines=42 bytes=980 hash=8df4f6b8b82a6b31;
symbol s_54824efcbf85b0a7 name="checkout" kind=function file=f_6c481624d8960b19 start=8:1 end=21:2 entry=false;
symbol s_721f31cc5ffeb935 name="reserve_inventory" kind=function file=f_6c481624d8960b19 start=23:1 end=30:2 entry=false;
workflow w_8feec23b6a19d218 kind=http framework="axum-compatible" trigger="POST /checkout" handler="checkout" entry=s_54824efcbf85b0a7 file=f_6c481624d8960b19 confidence=exact start=34:5 end=34:53;
attention workflow=w_8feec23b6a19d218 source="susumu:derived" score=79 detail="workflow trigger observed; handler symbol resolved; HTTP route observed; accepted expectation linked";
flow s_54824efcbf85b0a7 -> s_721f31cc5ffeb935 call="reserve_inventory" confidence=exact start=12:5 end=12:29;
flow s_54824efcbf85b0a7 -> ? call="charge_gateway" confidence=external start=15:5 end=15:31;
expectation e_91bbd1 target=workflow subject=w_8feec23b6a19d218 status=accepted source="human:product" title="Charge only after inventory is reserved" detail="The checkout workflow must reserve inventory before charging the customer.";
verification v_checkout_order expectation=e_91bbd1 status=passed method="cargo test checkout_order" source="ci:github-actions" evidence="run:123456" basis=3a834e7a4f2d901c detail="The checkout order test passed in CI.";
decision d_release_exception target=workflow subject=w_8feec23b6a19d218 status=accepted source="human:director" basis=3a834e7a4f2d901c title="Accept checkout exception" detail="The team accepts this implementation exception for the current release with follow-up verification required.";
work wk_checkout_agent target=workflow subject=w_8feec23b6a19d218 expectation=e_91bbd1 kind=implementation status=completed source="agent:codex" evidence="commit:abc123" title="Update checkout reservation" detail="Updated checkout so inventory reservation happens before payment capture.";
```

Every statement ends in `;`, so insignificant whitespace and newlines can be removed. Readable and minified artifacts use the same grammar and parser.

See [the artifact contract](docs/artifact.md), [the product architecture](docs/vision.md), [the Susumu vernacular](docs/vernacular.md), and [The Susumu Way](docs/the-susumu-way.md) for the boundary between today's scanner and the broader vision.

## Roadmap

- Deepen deterministic adapters for Rust, PHP, Python, JavaScript, TypeScript, TSX, and Vue.
- Add more workflow types: jobs, queues, events, tests, database boundaries, policies, and deployment checks.
- Deepen threaded reviews with server-derived timestamps, richer ownership, source revisions, and migration support.
- Improve dirty/stale review detection so changed code automatically flags affected expectations, verifications, and decisions across history.
- Expand the live stakeholder portal with richer workflow narratives, accessibility polish, permissions management, and broader timeline/search resources.
- Keep AI optional and bring-your-own-key. Generated summaries or draft records should be labeled, cited, and reviewable before becoming trusted project memory.

## Status

Susumu is pre-1.0 software, but the core product loop is now end to end: scanner -> evidence model -> portable `.susu` records -> CLI/TUI/static HTML/live portal/API/CI consumers. The deterministic evidence model and review boundaries remain the source of truth while collaboration, history, adapters, and deployment hardening continue to mature.

## License

MIT
