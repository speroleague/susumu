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
| JavaScript/TypeScript/TSX/Vue | functions, methods, imports, calls | Express-compatible `app`/`router` methods; React Router `<Route>` and React Navigation `<*.Screen>` navigation targets; Vue script blocks use the TypeScript/TSX grammar |

React and React Native share the JavaScript/TypeScript/TSX adapters. `.jsx` files use the JavaScript grammar and `.tsx` files use the TSX grammar. In addition to Express-compatible HTTP routes, these adapters emit two workflow kinds for front-end code:

**`navigation` workflows** — declarative routing:

- **React Router** — `<Route path="/users" element={<Users />} />` (and `component={Users}`), including `<Route index .../>`, which is recorded with the path `(index)`.
- **React Navigation** — `<Stack.Screen name="Home" component={HomeScreen} />` and the `Tab`/`Drawer` equivalents, keyed by the screen `name`.

A `navigation` workflow carries a `ROUTE <path>` or `SCREEN <name>` trigger and resolves the `component`/`element`/`getComponent` attribute to a handler symbol. Object-configuration routers (`createBrowserRouter([...])`, `useRoutes([...])`) and dynamically built route tables remain a visible gap rather than a guess.

**`component` workflows** — components that are also module entry points, for React apps that have no router at all:

- **React** — a function or arrow-assigned `const` that is exported at its declaration, is named in `PascalCase`, and returns JSX becomes a `COMPONENT <Name>` workflow whose handler is its own symbol. Non-exported helper components, lowercase render helpers, and non-JSX exports stay ordinary function symbols. Components exported apart from their declaration (`export { Foo }`) are not detected.
- **Vue** — every `.vue` single-file component is a component by file definition (a `<script setup>` block declares no component object), so each SFC yields one `COMPONENT <file-stem>` workflow.

An adapter is deterministic. It may return incomplete or ambiguous evidence, but it may not guess silently. Unsupported metaprogramming, dependency injection, reflection, macros, or dynamic dispatch remains visible as a gap until a more specific adapter, runtime trace, configuration reader, or explicit declaration resolves it.

Future adapters should be independently fixture-tested against representative framework code. Adding a language must not require changes to `.susu` consumers unless it introduces a genuinely new evidence concept.
