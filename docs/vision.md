# Product architecture

Susumu is a shared, evidence-backed memory of a software project.

It should answer different questions for different people without producing different versions of reality:

- Engineers and AI agents: Where is this implemented? What calls it? What changed? What remains unresolved? What evidence supports the claim?
- Product and business stakeholders: What workflow does the business operate? What is expected? Is it implemented and verified? What decision is blocked?
- Reviewers and leaders: What was done by a person or agent? What was reviewed? What risk, disagreement, or missing evidence remains?

## Four information planes

### 1. Observed system

Deterministic adapters collect files, symbols, routes, schemas, calls, events, jobs, tests, deployments, and runtime traces. Every observation records its source and confidence. Version 0.1 starts here with static call evidence.

Susumu may rank workflows by an evidence-based attention score: observed routes, resolved handlers, fan-out, unresolved call edges, linked expectations, verification results, and linked findings. This is not business priority. It is a transparent "look here first" heuristic until humans or imported systems provide real ownership, risk, revenue, or policy priority.

### 2. Declared intent

People connect expectations, business rules, policies, acceptance criteria, ownership, and architectural decisions to observed workflows. Intent is explicitly authored or imported; it is never confused with implementation evidence.

### 3. Work and verification

Human and AI activity records explain what changed, why it changed, which expectation it addresses, and how it was checked. Diffs, commits, test runs, analysis runs, and deployment results provide evidence. Agent-produced work carries agent provenance.

### 4. Review and decision

Comments, questions, approvals, exceptions, unresolved disagreements, and decisions attach to any stable target in the model. Susumu's current artifact supports authored threaded review records with replies, lifecycle status, and ownership labels; decisions remain first-class records so workflows can be reviewed as stacked evidence: implementation observations, authored expectations, verification results, and the human or policy judgment made at that point in time. This plane turns static documentation into an organizational feedback loop.

## Product surfaces

### Engineering workbench

The Rust TUI is fast, local, keyboard-driven, source-specific, and comfortable in an engineer or agent workflow. It favors density, exact locations, confidence, gaps, and CI-friendly export.

### Stakeholder web experience

The web application is a first-class product, not a browser skin for the TUI. A useful shorthand is "Swagger documentation for the entire system," expanded into a modern workflow and decision portal.

It should provide:

- progressive disclosure from business workflow to technical evidence;
- narrative workflow pages with success, failure, and exception paths;
- expectation pages showing status, implementation evidence, verification, ownership, and open decisions;
- timelines of human and agent activity;
- review threads anchored to workflows, expectations, decisions, or evidence;
- clear confidence and freshness indicators without exposing parser jargon by default;
- excellent search, responsive layouts, accessible interactions, transitions, and presentation quality suitable for company-wide use.

Both surfaces consume the same versioned `.susu` model. Presentation state may differ; factual state may not.

## AI boundary

The core product does not require AI.

Optional bring-your-own-key AI can help with summarization, naming, clustering, cross-language hypotheses, natural-language querying, and suggested missing expectations. Its output must be labeled as generated, retain provider/model provenance, cite underlying evidence, and require deterministic confirmation or human acceptance before becoming trusted project knowledge.

Source code and artifacts remain local unless a user explicitly configures a provider and sends selected context.

In a company workflow, AI can still be valuable as a record-keeping assistant: draft expectation updates from tickets, summarize decisions from reviews, propose intent records from pull requests, and flag places where a change may invalidate an earlier judgment. Those records should enter Susumu as drafts or review items, not silent facts. The durable system of record remains the `.susu` artifact plus explicit human, CI, policy, or deterministic scanner provenance.

## Delivery sequence

1. Prove the evidence loop: repository scanner, `.susu` artifact, engineering TUI.
2. Expand the initial framework-aware HTTP adapters into queues/events, jobs, database boundaries, and tests.
3. Extend deterministic identifiers with source-revision provenance and migration support.
4. Expand authored expectations and verification into decisions, work records, and review records.
5. Build the stakeholder web experience on the same artifact and local server.
6. Add CI comparison, freshness, policy checks, and agent-oriented query/update commands.
7. Add optional BYOK assistance only where deterministic analysis and human input are insufficient.
