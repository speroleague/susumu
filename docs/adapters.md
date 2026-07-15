# Language and framework adapters

Susumu separates shared analysis mechanics from language and framework knowledge.

The shared Rust core owns directory traversal, ignore behavior, safety limits, deterministic ids, evidence resolution, findings, `.susu` serialization, and consumer-facing models. The parser facade lives in `src/language.rs`; language-specific grammar, symbol, dependency, call, and workflow rules live behind the `LanguageAdapter` boundary in `src/language/adapters.rs`.

An adapter owns the facts that vary by ecosystem:

- Tree-sitter grammar selection;
- file extensions and entrypoint conventions;
- function, method, import, and call node shapes;
- framework triggers such as routes, jobs, events, and tests;
- ecosystem-specific confidence rules.

The initial adapters are:

| Adapter | Baseline evidence | Initial HTTP conventions |
| --- | --- | --- |
| Rust | functions, methods, `use`, calls | Axum-compatible routes, Actix Web attributes |
| PHP | functions, methods, namespace uses/includes, calls | Laravel `Route::...`, Symfony `#[Route]` |
| Python | functions, imports, calls | FastAPI-style method decorators, Flask `route` |
| JavaScript/TypeScript/TSX | functions, methods, imports, calls | Express-compatible `app`/`router` methods |

An adapter is deterministic. It may return incomplete or ambiguous evidence, but it may not guess silently. Unsupported metaprogramming, dependency injection, reflection, macros, or dynamic dispatch remains visible as a gap until a more specific adapter, runtime trace, configuration reader, or explicit declaration resolves it.

Future adapters should be independently fixture-tested against representative framework code. Adding a language must not require changes to `.susu` consumers unless it introduces a genuinely new evidence concept.
