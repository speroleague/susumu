# Susumu project context

Read this file at the beginning of every task in this repository. It is the durable context for how we work, what Susumu is becoming, and how changes should be made.

## What Susumu is

Susumu is a project-memory and traceability tool. It sits between source control, engineering quality tools, project-management work, and business decisions. It scans existing projects, records what can be observed, connects expectations to evidence, and presents the current state in forms that both engineers and non-technical stakeholders can use.

The key insight is that Susumu is not a replacement programming language and does not ask every team to rewrite its systems. It adds an understandable, portable layer of explanation and review on top of ordinary projects. The `.susu` files are the durable evidence; the TUI and HTML portal are views of that evidence.

## Long-term aspiration

Susumu should become a trustworthy bridge between:

- business intent, decisions, expectations, and acceptance;
- implementation work, source code, and Git history;
- static observations, review status, and verification evidence;
- human review at every skill level and AI-agent accountability.

Decisions and workflows should be able to stack and drill down. A reviewer should be able to move from a high-level expectation to the supporting records, then to a file and source location with syntax highlighting. Changes that may invalidate a prior decision or verification should become visibly dirty and request renewed review.

AI may eventually help summarize or propose links, but the core scanner and evidence model must work without AI. If AI integrations are added, they must be optional, open-source friendly, and use keys supplied by the project owner.

## Current product shape

- Rust CLI and TUI for engineers.
- Language adapters, including Rust and PHP, with room for more adapters.
- `.susu` packets for portable project evidence and review state.
- Expectations authored by people; Susumu derives observations, workflows, links, and review signals.
- Verifications that explain how an expectation was checked and what evidence supports it.
- Git import/linking so Susumu can connect findings to commits and later compare history.
- `check --json` and other machine-readable surfaces for agents and automation.
- Standalone HTML review portal, with independent panes, source views, syntax highlighting, traceability, and calm accessible styling.
- Optional `susumu.toml` project configuration for portal branding. Branding changes the presentation shell; it must not alter or hide evidence.
- GitHub Pages and internal hosting are intended deployment paths for continuously available project memory.

## Working principles

1. Represent what Susumu can currently know. Do not invent intent, certainty, timestamps, or verification.
2. Expectations are authored inputs. Evidence, findings, and proposed relationships are derived and must remain inspectable.
3. Make important workflows visible first, while preserving drill-down to the exact record, file, line, and code.
4. Prefer small, memorable commands and sensible conventions. The common path should feel as easy as Git or jj.
5. Keep packets portable and views replaceable. A local TUI, HTML export, hosted portal, and future integrations should read the same evidence.
6. Treat cleanliness and maintainability as product behavior: split functionality, keep boundaries clear, and flag code that increases review surface.
7. A change to code, decisions, expectations, or links can make related evidence dirty. Dirty state should be explainable and actionable.
8. Human review remains authoritative. Automation can point, summarize, and test; it should not silently claim acceptance.
9. Favor soft, readable, low-fatigue visual design. Support project branding through configuration without making the default experience noisy.
10. Never add an AI co-author, sign-off, or attribution on commits.

## Our development workflow

For each meaningful change:

1. Start by reading this file, the relevant expectations, and the current review/readiness output.
2. State the intended behavior in a focused expectation when the work introduces a durable product promise.
3. Implement the smallest coherent change, preserving existing user work and unrelated edits.
4. Add or update tests and documentation alongside the implementation.
5. Run formatting, focused tests, the full locked test suite, Clippy with warnings denied, and a Susumu review/readiness check appropriate to the change.
6. Use Susumu to record the implementation or verification link when the change has a durable relationship to an expectation.
7. Commit with one short conventional-commit subject that explains the change. Do not add co-authors or sign-offs.
8. Tell the project owner what changed, what was verified, what command they can run, and when to push.

Useful baseline checks:

```text
cargo fmt --all
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo run --locked -- review
cargo run --locked -- readiness
```

## What we are working toward next

The near-term priority is making the current system pleasant and dependable: clear commands, reliable adapters, strong evidence links, readable portal layouts, syntax-highlighted source context, config validation, and documentation that teaches the normal workflow. After that, deepen historical Git comparison, dirty-state propagation, decision/workflow drill-down, comments and review collaboration, and stable machine-readable APIs for agents.

Every feature should answer: what does this make easier to understand, verify, maintain, or decide—and what evidence will Susumu show for it?
