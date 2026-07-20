# The `.susu` artifact contract

`.susu` is the portable boundary between evidence producers and product experiences.

Producers include language scanners, framework adapters, source-control readers, test runners, CI systems, requirement importers, humans, and optional AI assistants. Consumers include the engineering TUI, CI checks, agent tools, exports, and the future stakeholder web application.

## Version 1 records

Version 1 contains observed evidence plus explicit authored intent:

- `project` - project identity and scan time.
- `file` - a supported source file, measured size, and optional content hash.
- `symbol` - a parsed module, function, or method with source range.
- `dependency` - a parsed import or use declaration.
- `workflow` - a framework-level trigger such as an HTTP route, its handler, and resolution evidence.
- `attention` - a deterministic attention score for a workflow, with source and reasons.
- `flow` - a call from one symbol to a resolved symbol or `?`.
- `expectation` - an authored requirement, policy, acceptance criterion, or business rule linked to a project, file, symbol, or workflow.
- `verification` - a recorded check of an expectation, including method, result, provenance, and optional evidence id.
- `decision` - an authored judgment, approval, rejection, exception, or open business choice linked to a project, file, symbol, or workflow.
- `work` - a recorded activity by a human, agent, import, or automation, optionally linked to an expectation and evidence reference.
- `finding` - a deterministic rule result with source, optional file link, and optional subject link.

Files, symbols, and workflows receive deterministic ids derived from stable source identity rather than scan order. Symbols also carry a fingerprint of their parsed source region, so verification and decision evidence for one symbol does not become stale when unrelated code in the same file changes. Older artifacts and targets without a narrower scope use the file-level fallback. A flow uses real arrow syntax:

```susu
flow s_14f9a710a831c89a -> s_325146f3bd3461ad call="authorize" confidence=exact start=44:5 end=44:27;
```

An unresolved edge retains the call and source evidence:

```susu
flow s_14f9a710a831c89a -> ? call="publish" confidence=external start=52:5 end=52:26;
```

## Workflow Attention

Susumu can rank workflows by an evidence-based attention score. This is not business priority and does not claim revenue, risk, or product importance. It is a deterministic hint for "look here first" based on what the artifact can see.

```susu
attention workflow=w_8feec23b6a19d218 source="susumu:derived" score=79 detail="workflow trigger observed; handler symbol resolved; HTTP route observed; accepted expectation linked";
```

Current signals include observed workflow triggers, resolved handler symbols, observed HTTP routes, fan-out, unresolved outgoing call edges, linked expectations, failed or inconclusive verification records, and linked findings.

The TUI and future web portal can put the highest-scoring workflows near the top while still showing the reasons behind the score.

Legacy artifacts that use the older `priority` record name are still accepted by the parser, but new artifacts write `attention`.

## Expectations

Expectations are authored records. Susumu may import them from humans, requirement files, issue trackers, policy systems, or optional AI-assisted drafts, but it does not infer them from code during a normal scan.

```susu
expectation e_91bbd1 target=workflow subject=w_8feec23b6a19d218 status=accepted source="human:product" title="Charge only after inventory is reserved" detail="The checkout workflow must reserve inventory before charging the customer.";
```

Expectation-only sidecar files are valid input for `--expectations`:

```susu
expectation e_docs target=project subject=- status=proposed source="human:ops" title="Document backup expectations" detail="The project should document backup and restore expectations.";
```

They can also be created without hand-writing syntax:

```powershell
cargo run -- expectation add --file expectations.susu --target project --title "Document backup expectations" --detail "The project should document backup and restore expectations."
```

Sidecars can be inspected and pruned from the CLI:

```powershell
cargo run -- expectation list --file expectations.susu
cargo run -- expectation remove --file expectations.susu e_docs
```

The authoring commands write only expectation sidecars. If the target file looks like a full `.susu` scan artifact, they refuse to overwrite it.

- `target` is `project`, `file`, `symbol`, or `workflow`.
- `subject` is the target id, or `-` for project-wide expectations. For file expectations, `expectation add --subject <path>` resolves a repository-relative path to its scanner id; `susumu resolve <path>` prints the id explicitly when scripting or reviewing a sidecar.
- `status` is `proposed`, `accepted`, or `superseded`.
- `source` records provenance, not authority. Examples: `human:product`, `policy:security`, `import:jira`, or `ai:draft`.

The review problem is intentionally explicit: later verification records can say whether implementation evidence appears to satisfy an expectation, but the expectation itself remains separate from observed code.

## Verifications

Verification records say how an expectation was checked. They are explicit evidence records, not inferred proof.

```susu
verification v_checkout_order expectation=e_91bbd1 status=passed method="cargo test checkout_order" source="ci:github-actions" evidence="run:123456" basis=3a834e7a4f2d901c detail="The checkout order test passed in CI.";
```

Verification-only sidecars can be created and managed from the CLI. A verification can optionally carry `--basis`; verifications without a basis are anchored to the checked expectation target fingerprint when merged into a scan artifact.

```powershell
cargo run -- verification add --file verifications.susu --expectation e_91bbd1 --status passed --method "cargo test checkout_order" --source ci:github-actions --evidence run:123456 --detail "The checkout order test passed in CI."
cargo run -- verification list --file verifications.susu
cargo run -- verification remove --file verifications.susu v_checkout_order
```

They can be merged into a scan alongside expectations:

```powershell
cargo run -- C:\path\to\project --expectations expectations.susu --verifications verifications.susu --output project.susu --headless
```

- `expectation` is the expectation id being checked.
- `status` is `passed`, `failed`, or `inconclusive`.
- `method` names the check, such as a test command, manual review, policy check, or trace inspection.
- `source` records provenance for the verification.
- `evidence` is an optional external or future `.susu` evidence id, or `-`.
- `basis` is an optional evidence fingerprint recorded when the verification was performed or merged. It fingerprints the current target of the checked expectation, not the verification text itself.

If a later scan observes that the checked expectation target fingerprint differs from the verification's recorded `basis`, Susumu marks the verification for review with `SUS023`. The verification status is not changed; the finding only says that the evidence the check was based on changed.

### What counts as verified

The scanner does not mark an expectation as verified because names are close, a route looks related, or an implementation seems plausible. Static source evidence can link an expectation to a workflow, show relevant code, and surface gaps, but that is not proof that the expectation is satisfied.

An expectation should be considered verified only when a verification producer records a check result. Examples include a CI test run, policy engine, manual review, runtime trace, deployment check, or future deterministic Susumu check. Susumu can suggest candidate links from ids, names, paths, comments, commits, or test metadata, but suggested links should remain `proposed` or `inconclusive` until accepted by a person or confirmed by a deterministic check.

This gives Susumu a clean trust boundary:

- scanner observations say "this code exists here";
- expectations say "someone or something expects this";
- verifications say "this check reported this result";
- decisions say "this judgment was made on this evidence";
- review findings say "this relationship changed, failed, or needs attention."

Susumu validates expectation links when artifacts are scanned or opened:

- `SUS010` - a file, symbol, or workflow expectation is missing a `subject` id.
- `SUS011` - an expectation points at an id that is not present in the current artifact.
- `SUS012` - a project-wide expectation carries a subject id even though it should use `subject=-`.
- `SUS020` - a verification points at an expectation id that is not present in the current artifact.
- `SUS023` - a verification's recorded basis differs from the current fingerprint of the checked expectation target.

## Decisions

Decision records capture authored judgment. They are where approvals, rejected proposals, temporary exceptions, tradeoffs, and unresolved choices become inspectable project memory. They do not prove implementation behavior; they explain what a person, policy process, or imported system decided about a target.

```susu
decision d_release_exception target=workflow subject=w_8feec23b6a19d218 status=accepted source="human:director" basis=3a834e7a4f2d901c title="Accept checkout exception" detail="The team accepts this implementation exception for the current release with follow-up verification required.";
```

Decision-only sidecars can be created and managed from the CLI:

```powershell
cargo run -- decision add --file decisions.susu --target workflow --subject w_8feec23b6a19d218 --status accepted --source human:director --title "Accept checkout exception" --detail "The team accepts this implementation exception for the current release with follow-up verification required."
cargo run -- decision list --file decisions.susu
cargo run -- decision remove --file decisions.susu d_release_exception
```

They can be merged into a scan alongside expectations and verifications:

```powershell
cargo run -- C:\path\to\project --expectations expectations.susu --verifications verifications.susu --decisions decisions.susu --output project.susu --headless
```

- `target` is `project`, `file`, `symbol`, or `workflow`.
- `subject` is the target id, or `-` for project-wide decisions.
- `status` is `proposed`, `accepted`, `rejected`, or `superseded`.
- `source` records provenance, not authority. Examples: `human:director`, `human:architect`, `policy:security`, or `import:jira`.
- `basis` is an optional evidence fingerprint recorded when the decision was made or merged. Decisions without a basis are anchored to the current target fingerprint when merged into a scan artifact.

If a later scan observes that a decision's current target fingerprint differs from its recorded `basis`, Susumu marks the decision for review with `SUS033`. The decision status is not changed; the finding only says the evidence it was based on changed.

Susumu validates decision links when artifacts are scanned or opened:

- `SUS030` - a file, symbol, or workflow decision is missing a `subject` id.
- `SUS031` - a decision points at an id that is not present in the current artifact.
- `SUS032` - a project-wide decision carries a subject id even though it should use `subject=-`.
- `SUS033` - a decision's recorded basis differs from the current target fingerprint.

## Work

Work records describe activity. They can say that a person implemented a feature, an AI agent changed a workflow, a reviewer inspected a path, an import found a related commit, automation performed documentation work, or CI/configuration infrastructure changed. They are activity history, not verification proof.

```susu
work wk_checkout_agent target=workflow subject=w_8feec23b6a19d218 expectation=e_91bbd1 kind=implementation status=completed source="agent:codex" evidence="commit:abc123" title="Update checkout reservation" detail="Updated checkout so inventory reservation happens before payment capture.";
```

Work-only sidecars can be created and managed from the CLI:

```powershell
cargo run -- work add --file work.susu --target workflow --subject w_8feec23b6a19d218 --expectation e_91bbd1 --kind implementation --status completed --source agent:codex --evidence commit:abc123 --title "Update checkout reservation" --detail "Updated checkout so inventory reservation happens before payment capture."
cargo run -- work list --file work.susu
cargo run -- work remove --file work.susu wk_checkout_agent
```

They can be merged into a scan alongside expectations, verifications, and decisions:

```powershell
cargo run -- C:\path\to\project --expectations expectations.susu --verifications verifications.susu --decisions decisions.susu --work work.susu --output project.susu --headless
```

- `target` is `project`, `file`, `symbol`, or `workflow`.
- `subject` is the target id, or `-` for project-wide work.
- `expectation` is an optional expectation id that the work claims to address, or `-`.
- `kind` is `implementation`, `verification`, `documentation`, `infrastructure`, `review`, or `other`.
- `status` is `proposed`, `in_progress`, `completed`, `blocked`, or `superseded`.
- `source` records provenance, such as `human:engineer`, `agent:codex`, `import:git`, or `automation:ci`.
- `evidence` is an optional external or future `.susu` evidence id, such as a commit, PR, ticket, or agent run.

Susumu validates work links when artifacts are scanned or opened:

- `SUS040` - a file, symbol, or workflow work record is missing a `subject` id.
- `SUS041` - a work record points at an id that is not present in the current artifact.
- `SUS042` - a project-wide work record carries a subject id even though it should use `subject=-`.
- `SUS043` - a work record points at an expectation id that is not present in the current artifact.

## Findings

Findings are deterministic rule results, not human review comments. The `source` field records which producer emitted the finding. Current scanner-generated findings use `susumu:scanner` for direct scan observations and `susumu:derived` for deterministic relationship or attention analysis.

```susu
finding SUS004 source="susumu:derived" severity=info title="Ambiguous call targets" detail="2 calls matched multiple symbols. Targets remain unresolved; no target was selected." file=- subject=-;
```

## Confidence

- `exact` - exactly one symbol with that name exists in the caller's file.
- `likely` - exactly one symbol with that name exists in the scanned project.
- `ambiguous` - multiple symbols are plausible and Susumu did not choose.
- `external` - no scanned project symbol matches; the target may be a library, framework, generated symbol, or dynamic dispatch.

These terms describe resolution evidence, not business correctness.

## Minification

Semicolons delimit all statements. Whitespace outside quoted strings is insignificant, including around `->`. Minification joins records and removes presentation newlines; it does not create a second binary format. The normal parser reads both forms.

This keeps artifacts inspectable, diffable, streamable, and easy for agents to exchange. A future binary transport can be added for scale without replacing `.susu` as the review format.

## Evolution rules

The artifact begins with `susu version=N;`. Additive records and fields must preserve old meanings. A breaking semantic change increments the version and requires an explicit migration.

Future records will cover threaded reviews. Those records must carry provenance and target links. They will not be inferred into existence merely because an AI model produced plausible prose.
