# Contributing to Susumu

Susumu is early, but its trust model should stay clear from the beginning: observed code evidence, authored intent, verification results, decisions, and AI-assisted drafts are different kinds of statements. Please keep those boundaries visible in code, docs, and UI text.

## Development setup

Install Rust 1.88 or newer, then run:

```powershell
cargo fmt --all
cargo test
cargo clippy --all-targets -- -D warnings
```

Use Susumu while changing Susumu:

```powershell
cargo run -- review
cargo run -- status
cargo run -- git --since origin/main
cargo run -- review
cargo run -- open
```

Before a pull request, the useful habit is:

1. Update `expectations.susu` when the change adds or clarifies project intent.
2. Make the code or docs change.
3. Commit with a single-line conventional commit message.
4. Run `cargo run -- git --since origin/main` to connect the commit to expectations.
5. If Susumu reports a valid commit as unconnected, run `cargo run -- git link <commit> <expectation-id>` to record the relationship explicitly.
6. Run `cargo run -- review` so the generated packet includes that work.
7. Use `cargo run -- status` or `cargo run -- open --summary` to inspect what still needs review.

Generated `.susu`, `.review.susu`, HTML, and target files should usually stay out of commits unless they are intentional fixtures or examples.

## Design rules

- Keep scanner output deterministic. If Susumu cannot prove a relationship, record ambiguity instead of guessing.
- Keep AI optional. Any generated text should be labeled, cite underlying evidence, and remain reviewable.
- Prefer stable ids and portable records. A `.susu` artifact should be useful after it leaves the source tree.
- Do not turn static evidence into business truth. Code can show behavior; expectations show intent; verifications report checks; decisions record judgment.
- Use scanner language carefully: observed, resolved, unresolved, linked, matched, reported. Avoid human-sounding blame words for deterministic findings.

## Adding language or framework support

Language-specific behavior belongs behind the adapter boundary in `src/language/adapters.rs` unless a genuinely new evidence concept is needed. New adapters or framework rules should include fixtures that demonstrate:

- symbols and imports are extracted correctly;
- workflow triggers point at the right handler when possible;
- ambiguous, external, generated, or dynamic edges remain visible;
- generated `.susu` records remain readable and stable across rescans.

## Useful docs

- [Artifact contract](docs/artifact.md)
- [Language and framework adapters](docs/adapters.md)
- [Product architecture](docs/vision.md)
- [The Susumu Way](docs/the-susumu-way.md)
- [Susumu vernacular](docs/vernacular.md)
