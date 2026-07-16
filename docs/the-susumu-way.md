# The Susumu Way

Susumu works best when it becomes a small habit around normal development, not a ceremony.

The goal is simple: keep business intent, implementation evidence, verification, decisions, and work history close enough that humans and agents can understand why the project is the way it is.

## The short version

Use these commands most of the time:

```powershell
cargo run -- review
cargo run -- status
cargo run -- git --since main
cargo run -- review
cargo run -- open
```

Installed globally, that becomes:

```powershell
susumu review
susumu status
susumu git --since main
susumu review
susumu open
```

## Which files people edit

People edit authored sidecars:

- `expectations.susu` for what should be true.
- `verifications.susu` when checks are recorded outside the generated artifact.
- `decisions.susu` when business or engineering judgment should be preserved.
- `work.susu` only when work records are intentionally authored by hand.

The most important one is `expectations.susu`. Start there. A project does not need perfect records on day one.

## Which files Susumu writes

The easy workflow writes generated files under `.susumu/`:

- `.susumu/project.susu` is the current generated project artifact.
- `.susumu/review.susu` is the portable review packet.
- `.susumu/check.json` is the machine-readable status report.
- `.susumu/review.html` is the standalone portal export.
- `.susumu/work.susu` is generated when `susumu git` exports Git-connected work.

Generated files should usually stay out of commits. Attach or publish them when they are useful as review artifacts, release snapshots, or business decision records.

## Day-to-day engineering flow

Before starting work:

```powershell
susumu status
```

This shows the current review queue: stale records, missing verification, scanner findings, unresolved workflow gaps, and important workflows.

After making commits:

```powershell
susumu git --since main
susumu review
```

The first command connects commits to expectations and exports work records. The second command folds that work back into the review packet.

If Susumu cannot safely infer which expectation a commit supports, link it explicitly:

```powershell
susumu git link abc123 e_susumu_docs_teach_daily_workflow --kind documentation
susumu review
```

This writes a work record without rewriting Git history.

When possible, `susumu git` will suggest likely expectations and print ready-to-copy `susumu git link ...` commands under unconnected commits. Treat those as suggestions, not facts; choose the expectation that actually matches the intent of the work.

Before opening a pull request:

```powershell
susumu review
susumu open
```

Use the portal to inspect the top workflows, expectation support, review items, and source previews.

## Agent-assisted development flow

An AI agent should treat Susumu as project memory, not as a place to hide uncertainty.

A good agent loop is:

1. Read `expectations.susu`.
2. Make the code or docs change.
3. Use a conventional commit message.
4. Run `susumu git --since <base>`.
5. If the commit is unconnected but the intent is known, run `susumu git link <commit> <expectation-id>`.
6. Run `susumu review`.
7. Report which expectation the work supported and what still needs verification.

Agents should not claim an expectation is satisfied just because they changed code. Work supports an expectation. Verification checks it. Decisions record judgment about it.

## Business and stakeholder flow

Business users should not need to read code or run long commands.

The stakeholder path is:

```powershell
susumu open
```

The portal should answer:

- What are the important workflows?
- What does the system appear to do?
- What expectations exist?
- What work has been done for those expectations?
- What has been verified?
- What decisions or exceptions were recorded?
- What changed and may need review?

Long term, this is where comments, approvals, questions, release snapshots, and decision history should live.

## Review artifacts as points in time

A review packet is a snapshot. It is useful when a team needs to ask:

- What did we believe at this point?
- What did the code show at this point?
- What expectations had support?
- What expectations lacked verification?
- What decisions were made with the evidence available then?

That makes Susumu a bridge between Git history and business history.

## What Susumu can and cannot prove

Susumu can deterministically observe many things:

- files;
- symbols;
- imports;
- calls;
- detected workflows;
- route handlers;
- changed files in Git commits;
- explicit links to expectations, work, verifications, and decisions.

Susumu can report support:

- a target exists;
- work is linked;
- verification records exist;
- decisions are linked;
- scanner findings affect the target.

Susumu should not silently convert support into proof. A work record is not a passed test. A matching commit message is not a business approval. A route observed by a scanner is not a complete runtime guarantee.

That boundary is the trust model.

## When to use advanced commands

Use the advanced commands when you need explicit control:

- CI needs a specific artifact path.
- A pull request needs an uploaded HTML portal.
- A test fixture needs stable output.
- You want to compare two review packets.
- You want to inspect an older Git ref with `git rewind`.
- You want to import Git work at file or workflow depth.

The advanced commands are still part of Susumu. They are the plumbing. The daily commands are the front door.
