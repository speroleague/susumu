# Susumu

Susumu makes an existing software project explainable.

It sits between source control, specifications, reviews, and business decisions. Point it at a repository and it builds a deterministic evidence model of source files, symbols, dependencies, workflows, call flows, ambiguity, expectations, verifications, decisions, and work records. The result can be saved as a portable `.susu` artifact and reopened without the source tree.

The first release is a local-first Rust scanner, terminal workbench, review packet generator, and standalone web review surface. The long-term goal is a shared project memory for engineers, AI agents, business stakeholders, and reviewers: what the system is, what it does, what is expected of it, what work addressed those expectations, and what people have decided or questioned.

## Principles

- Evidence before explanation. A source location, parser result, test result, commit, or explicit declaration backs every factual claim.
- Ambiguity stays visible. Susumu records unresolved and dynamic calls as gaps instead of inventing connections.
- One model, multiple experiences. The engineering TUI and the future stakeholder web application consume the same `.susu` artifact.
- Local-first and open source. The first scanner needs no service, account, or AI key.
- AI is optional. Bring-your-own-key AI may later summarize or propose hypotheses, but its output must be labeled and may not silently become observed fact.

## Current capabilities

- Scans Rust, PHP, Python, JavaScript, TypeScript, and TSX through language-specific adapters.
- Keeps per-language parser behavior behind `src/language/adapters.rs`, so new ecosystems can be added without changing `.susu` consumers.
- Respects `.gitignore`, `.ignore`, Git excludes, hidden-directory filtering, and a 2 MiB per-file safety limit.
- Extracts modules, functions, methods, imports, and calls using Tree-sitter.
- Detects initial HTTP workflows for Express-compatible, FastAPI-compatible, Flask-compatible, Laravel, Symfony, Axum-compatible, and Actix Web conventions.
- Ranks workflows with deterministic attention scores and explicit reasons.
- Uses deterministic path/name-based ids so evidence and review targets survive ordinary rescans.
- Resolves same-file calls exactly and unique project-wide calls as likely.
- Preserves ambiguous and external calls as visible gaps.
- Reports deterministic findings for large files, long workflow units, high fan-out, ambiguous targets, parse recovery, and call cycles.
- Carries authored expectations as explicit intent records linked to projects, files, symbols, or workflows.
- Flags expectation records that are missing subjects or point at stale evidence ids.
- Carries verification records that say how expectations were checked, with status, method, source, and evidence.
- Carries decision records that capture authored judgment, approvals, exceptions, and unresolved business choices.
- Carries work records that explain what humans, agents, imports, or automation changed or reviewed.
- Builds a Review queue from stale decisions, failed or inconclusive verification records, scanner findings, and unresolved workflow gaps.
- Explores overview metrics, review items, expectations, verifications, decisions, work records, detected workflows, call flows, findings, and files in a Ratatui interface.
- Reads and writes the versioned `.susu` syntax in readable or minified form.

This first model is a call-flow model, not yet full variable-level data lineage. That distinction matters: Susumu should grow its evidence carefully rather than overstate what static analysis can prove.

## Quick start

Rust 1.88 or newer is required.

Generate the demo artifact:

```powershell
cargo run -- .\examples\demo-project --expectations .\examples\demo-project\expectations.susu --verifications .\examples\demo-project\verifications.susu --decisions .\examples\demo-project\decisions.susu --work .\examples\demo-project\work.susu --output .\target\susumu-demo.susu --headless
```

Open the engineering TUI:

```powershell
cargo run -- .\target\susumu-demo.susu
```

Create a review packet and open the local web portal:

```powershell
cargo run -- review build .\examples\demo-project --expectations .\examples\demo-project\expectations.susu --verifications .\examples\demo-project\verifications.susu --decisions .\examples\demo-project\decisions.susu --work .\examples\demo-project\work.susu --artifact-output .\target\susumu-demo.susu --output .\target\susumu-demo.review.susu --html .\target\susumu-demo.html
cargo run -- review serve .\target\susumu-demo.review.susu
```

Or create/export from an existing artifact:

```powershell
cargo run -- review create .\target\susumu-demo.susu --output .\target\susumu-demo.review.susu
cargo run -- review export-html .\target\susumu-demo.review.susu --output .\target\susumu-demo.html
```

Scan your own project:

```powershell
cargo run -- init C:\path\to\project --name "My Project"
cargo run -- C:\path\to\project --output project.susu --headless
cargo run -- project.susu
```

## Command reference

```powershell
cargo run -- C:\path\to\project
```

Start a project the Susumu way by creating an authored expectations sidecar:

```powershell
cargo run -- init C:\path\to\project --name "Checkout Service"
```

Scan a repository, write an artifact, and skip the TUI:

```powershell
cargo run -- C:\path\to\project --output project.susu --headless
```

Merge authored expectations while scanning. Initialized repositories automatically load `expectations.susu`; use `--expectations` when the sidecar lives somewhere else:

```powershell
cargo run -- C:\path\to\project --expectations expectations.susu --output project.susu --headless
```

Merge expectations, verifications, decisions, and work together:

```powershell
cargo run -- C:\path\to\project --expectations expectations.susu --verifications verifications.susu --decisions decisions.susu --work work.susu --output project.susu --headless
```

Create or update an expectation-only sidecar:

```powershell
cargo run -- expectation add --file expectations.susu --target project --source human:product --title "Keep architecture explainable" --detail "The project should keep scanner evidence, authored intent, and review feedback in the same portable artifact."
```

List or remove authored expectations:

```powershell
cargo run -- expectation list --file expectations.susu
cargo run -- expectation remove --file expectations.susu e_91bbd1
```

Create, list, or remove verification sidecars. A verification can optionally carry `--basis`; verifications without a basis are anchored to the checked expectation target fingerprint when merged into a scan artifact.

```powershell
cargo run -- verification add --file verifications.susu --expectation e_91bbd1 --status passed --method "cargo test checkout_order" --source ci:github-actions --evidence run:123456 --detail "The checkout order test passed in CI."
cargo run -- verification list --file verifications.susu
cargo run -- verification remove --file verifications.susu v_checkout_order
```

Create, list, or remove decision sidecars. A decision can optionally carry `--basis`; decisions without a basis are anchored to the current target fingerprint when merged into a scan artifact.

```powershell
cargo run -- decision add --file decisions.susu --target workflow --subject w_8feec23b6a19d218 --status accepted --source human:director --title "Accept checkout exception" --detail "The team accepts this implementation exception for the current release with follow-up verification required."
cargo run -- decision list --file decisions.susu
cargo run -- decision remove --file decisions.susu d_checkout_exception
```

Create, list, or remove work sidecars:

```powershell
cargo run -- work add --file work.susu --target workflow --subject w_8feec23b6a19d218 --expectation e_91bbd1 --kind implementation --status completed --source agent:codex --evidence commit:abc123 --title "Update checkout reservation" --detail "Updated checkout so inventory reservation happens before payment capture."
cargo run -- work list --file work.susu
cargo run -- work remove --file work.susu wk_checkout_agent
```

Import local Git commits as project-wide work records:

```powershell
cargo run -- git import --since main --output work.susu
cargo run -- git import --limit 25 --output work.susu
cargo run -- git import --since main --artifact project.susu --target-depth file --output work.susu
cargo run -- git import --since main --artifact project.susu --target-depth workflow --output work.susu
cargo run -- git import --since main --artifact project.susu --target-depth workflow --output work.susu --json
```

The importer reads local `git log`, creates one completed work record per commit, uses stable ids derived from commit hashes, records `evidence="commit:<sha>"`, and includes changed files in the work detail. Without `--artifact`, imports stay project-wide. With `--artifact`, `--target-depth file` targets exactly one matched artifact file, while `--target-depth workflow` targets exactly one workflow from the changed files; ambiguous commits stay file- or project-level instead of guessing. If a commit message or body mentions exactly one known expectation id, the imported work links `expectation=<id>` and may use that expectation's target when no more specific changed-file target is available. Add `--json` when an agent or CI job needs a machine-readable import report.

Connect Git commits to Susumu workflows and records:

```powershell
cargo run -- git connect --artifact project.susu --since main
cargo run -- git connect --artifact project.susu --limit 25 --json
cargo run -- git connect --artifact project.susu --since main --export-work work.susu
```

`git connect` is read-only unless `--export-work` is supplied. It correlates commits with the current artifact by changed workflow files, explicit Susumu ids in commit messages, expectation targets, verification/decision targets, and existing work records with `evidence="commit:<sha>"`. Commits with a matching work record are `connected`; commits that touch known Susumu context but do not have a work record are `needs_record`; commits with no visible Susumu relationship are `unconnected`. With `--export-work`, Susumu writes completed work records for `needs_record` commits using stable commit-derived ids, so reruns update the same records instead of duplicating them.

Compare the current artifact against code evidence from an older Git ref:

```powershell
cargo run -- git rewind --from HEAD~1 --artifact project.susu
cargo run -- git rewind --from main --artifact project.susu --json
cargo run -- git rewind --from main --artifact project.susu --old-output old-main.susu
```

`git rewind` reconstructs the selected ref into a temporary snapshot without checking out or mutating the repository, scans that snapshot, and runs the same comparison model as `diff`. This is the first bridge toward connecting Susumu records with historical Git work: it answers "what did the workflow evidence look like at that ref compared with the current artifact?"

Write the compact form:

```powershell
cargo run -- C:\path\to\project --output project.min.susu --minify --headless
```

Open an existing artifact:

```powershell
cargo run -- project.susu
```

Check an artifact or project for review blockers:

```powershell
cargo run -- check project.susu
cargo run -- check C:\path\to\project --expectations expectations.susu --verifications verifications.susu --decisions decisions.susu --work work.susu
cargo run -- check project.susu --json
```

`check` exits nonzero when critical review items are present, such as failed verifications. Add `--strict` to also fail on warnings such as stale targets, inconclusive verifications, blocked work, or changed verification/decision evidence.

Create a handoff brief for a human reviewer or the next agent:

```powershell
cargo run -- handoff project.susu
cargo run -- handoff project.susu --json
```

`handoff` turns the current Susumu artifact into a compact briefing: evidence counts, record counts, overall review result, the most important workflows Susumu can infer, review items, expectations that still need verification, completed work that needs verification, caveats, and suggested next actions. It is intentionally scanner-first and deterministic, so teams can use it without AI keys.

Create a point-in-time review packet:

```powershell
cargo run -- review build C:\path\to\project --artifact-output project.susu --output review.susu --check-json check.json --html review.html
cargo run -- review create project.susu --output review.susu
cargo run -- review create project.susu --json
```

`review build` is the day-to-day command for initialized projects. It scans the project, automatically loads `expectations.susu` when present, writes the current `.susu` artifact, creates the review packet, can write `check --json`, and can export the standalone HTML portal. Add `--serve` to open the local portal after building, or `--fail-on-check` when CI should fail after outputs are written.

Review packets include expectation support summaries. These summaries show whether each expectation's target is currently observed and which verification, work, decision, or finding records are linked. They do not prove the expectation is satisfied; they show what evidence currently supports or fails to support review.

To let real Git history support expectations, export work records from commits and include them in the next review build:

```powershell
cargo run -- git connect --artifact .\target\susumu-self.susu --since origin/main --export-work .\target\susumu-work.susu
cargo run -- review build . --work .\target\susumu-work.susu --artifact-output .\target\susumu-self.susu --output .\target\susumu-self.review.susu --check-json .\target\susumu-self-check.json --html .\target\susumu-self.review.html
```

`review create` packages an existing artifact or project together with the handoff summary, check result, review items, top workflows, caveats, next actions, and portable syntax-highlighted source snippets when the source files are readable. The packet uses JSON with `schema_version="susumu.review.v1"`, so it can be attached to pull requests, stored as a release decision snapshot, passed to an AI agent, or opened later by a TUI/web review surface. Creating a packet does not fail just because review issues are present; those issues are captured inside the packet.

Open or compare review packets later:

```powershell
cargo run -- review open review.susu
cargo run -- review open review.susu --tui
cargo run -- review diff old.review.susu new.review.susu
cargo run -- review diff old.review.susu new.review.susu --json
cargo run -- review serve review.susu
cargo run -- review export-html review.susu --output review.html
```

`review open` replays the saved packet summary without needing the original project directory. Add `--tui` to open the embedded artifact in the existing Susumu TUI for workflow, record, finding, and source drill-down. `review diff` compares two review snapshots: review result changes, review item changes, next-action changes, top-workflow changes, embedded artifact changes, and stale evidence in the newer packet. Add `--fail-on-regression` when CI should fail if the newer packet newly fails or has more critical review items.

`review serve` starts a local-only web portal, prints a localhost URL, and serves the packet as a modern single-page HTML/CSS/JavaScript review surface with clickable workflow drill-down, expectation traceability, linked verification/decision/work context, source preview panes, and embedded syntax highlighting. It has no frontend build step and no external assets. The server also exposes `/review.json` for tooling.

`review export-html` writes the same portal as a standalone HTML file. Use it when you want to attach the review to a pull request, send it to a stakeholder, archive a release decision, or open it without running a server.

Compare two artifacts:

```powershell
cargo run -- diff old.susu new.susu
cargo run -- diff old.susu new.susu --fail-on-stale
cargo run -- diff old.susu new.susu --json
```

`diff` compares files, workflows, expectations, verifications, decisions, and work records. It also lists stale verification or decision evidence detected in the newer artifact. Add `--fail-on-stale` when CI or an agent should stop if previously accepted checks or judgments need review.

Inside the TUI, use `1` through `9`, `0`, or `Tab` to change views, `j`/`k` or the arrow keys to move, `Enter` to jump from a Review or Connections item to its source record, `b` to go back, `e` to export readable syntax, `m` to export minified syntax, and `q` to quit. The `0` shortcut jumps to the final Files tab; use `Tab` or the arrow keys for any tab between `9` and `0`.

## Dogfooding Susumu

This repository uses Susumu to describe Susumu.

The root `expectations.susu` file is the authored intent: the project expectations humans want Susumu to preserve and review. When Susumu scans this initialized repo, it automatically loads that sidecar. The generated artifact, check JSON, review packet, and HTML portal are produced from the current repository state.

This repo has already been initialized with `expectations.susu`. Generate the self-review artifact:

```powershell
cargo run -- . --output .\target\susumu-self.susu --headless
```

Build and view the review packet:

```powershell
cargo run -- review build . --artifact-output .\target\susumu-self.susu --output .\target\susumu-self.review.susu --check-json .\target\susumu-self-check.json --html .\target\susumu-self.review.html
cargo run -- review serve .\target\susumu-self.review.susu
```

This is the intended loop for other projects too: keep expectations explicit, let Susumu observe the code, then review what the generated artifact says is implemented, unresolved, stale, or missing.

## CI and pull requests

This repository includes a GitHub Actions workflow at `.github/workflows/ci.yml`.

The `Rust checks` job runs formatting, tests, and Clippy as hard gates. The `Susumu self-review packet` job builds the repository's own `.susu` artifact, automatically loading `expectations.susu`, writes `check --json`, creates a `.review.susu` packet, exports the standalone HTML portal, and uploads those files as workflow artifacts.

The self-review `check --json` step records its exit code but does not fail the job, because review findings are useful output for the packet. In a production repository, use `cargo run -- check project.susu --strict` or `cargo run -- diff old.susu new.susu --fail-on-stale` when the review signal should block a pull request.

## Try the workflow demo

The Susumu self-scan is useful for checking parser coverage, but this repository is a CLI/TUI, so it does not currently produce many business workflows. The demo project is intentionally shaped like a small product system: TypeScript checkout routes, PHP Laravel-style routes, Rust Axum-style routes, authored expectations, verification records, decision records, and work records.

Build a rich demo artifact:

```powershell
cargo run -- .\examples\demo-project --expectations .\examples\demo-project\expectations.susu --verifications .\examples\demo-project\verifications.susu --decisions .\examples\demo-project\decisions.susu --work .\examples\demo-project\work.susu --output .\target\susumu-demo.susu --headless
```

Open it in the TUI:

```powershell
cargo run -- .\target\susumu-demo.susu
```

The Overview and Workflows tabs put the highest-attention workflows first. The score is deterministic: observed triggers, resolved handlers, fan-out, unresolved call edges, linked expectations, and linked verification records all contribute explicit reasons. The Review tab then turns the sharp edges into a navigable queue. Press `Enter` on a review item to inspect the underlying finding, verification, decision, work record, or workflow, then `b` to return.

The Connections tab groups work records into review-oriented buckets such as Git-connected work, work that needs verification, blocked review work, and unlinked work. Selecting a connection shows commit evidence, linked expectation context, verification status, and source preview when the work targets a workflow. Selecting a workflow also shows linked expectations, verification records, work records, and decisions in the workflow detail pane, so implementation evidence, business intent, check results, activity history, and authored judgment are visible together. Workflow, flow, finding, and file selections also show a source preview when the original source tree is available, with syntax highlighting powered by Syntect and the evidence line emphasized. The Overview and Files tabs show source availability so portable artifacts remain honest when opened away from their original repository.

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

See [the artifact contract](docs/artifact.md), [the product architecture](docs/vision.md), and [the Susumu vernacular](docs/vernacular.md) for the boundary between today's scanner and the broader vision.

## Roadmap

- Deepen deterministic adapters for Rust, PHP, Python, JavaScript, TypeScript, and TSX.
- Add more workflow types: jobs, queues, events, tests, database boundaries, policies, and deployment checks.
- Expand `.susu` with threaded reviews, comments, ownership, source revisions, and migration support.
- Improve dirty/stale review detection so changed code automatically flags affected expectations, verifications, and decisions.
- Build the stakeholder web experience as a polished workflow and decision portal on the same review packet model.
- Add CI and pull-request workflows for `check --json`, `diff --json`, `review create`, `review diff`, and Git-connected work records.
- Keep AI optional and bring-your-own-key. Generated summaries or draft records should be labeled, cited, and reviewable before becoming trusted project memory.

## Status

This is an early vertical slice. It proves the scanner -> evidence model -> `.susu` artifact -> TUI/web/review consumer loop. It is already useful for exploring code flow and project memory, but it should be treated as pre-1.0 software while the artifact model, adapters, review workflow, and web experience mature.

## License

MIT
