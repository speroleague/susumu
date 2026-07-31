# Collaboration backend contract

This document defines the first live collaboration boundary for Susumu. It is intentionally provider-aware for repositories and provider-neutral for users. The CLI, TUI, live frontend, and CI must remain different clients of the same project-memory model.

## Deployment shape

The supported deployment is an optional Docker Compose stack:

```text
susumu-web  ->  susumu-api  ->  postgres
                           ->  GitHub App installations
```

- `susumu-api` is a Rust service and the only component allowed to mutate collaboration state or use repository credentials.
- `susumu-web` serves the modern authenticated frontend through Caddy and communicates with the API over the same origin or an explicitly configured trusted origin.
- `postgres` stores users, repository connections, review events, synchronization state, and audit history.
- A deployment may also run the existing CLI and CI workflows against exported `.susu` artifacts. The live service is additive, not a replacement for the local toolchain.
- Secrets are provided through deployment configuration or a secret manager. They are never stored in `.susu` records, frontend bundles, or browser storage.

The first deployment does not require OAuth. Susumu has local users and roles. GitHub is the repository provider and is connected server-to-server by an administrator through a GitHub App installation. The first-repository onboarding wizard is skippable and is shown only to administrators when no repositories are connected.

## User and role model

Users are application identities, not GitHub identities. A user record contains an id, display name, login/email, password hash, active state, and creation/update metadata. Passwords are stored only as slow, salted password hashes.

Initial roles are:

- `admin`: manage users, repository connections, project policy, and synchronization settings;
- `reviewer`: search evidence and create discussion, questions, objections, approvals, and review actions;
- `owner`: the same review capabilities plus responsibility for assigned targets;
- `automation`: scoped API credentials for CI or service-to-service use.

Role names describe Susumu permissions. They do not imply that a person’s review is legally sufficient, that a control is met, or that a user has GitHub permissions outside Susumu.

An administrator is bootstrapped through deployment configuration or a one-time setup command. There is no public self-registration endpoint in the first deployment. User management is explicit and auditable.

## Repository connections

Each connected repository has:

- provider (`github` in the first implementation);
- organization/owner and repository name;
- allowed base branches;
- project identity and the complete supported Susumu sidecar set;
- the GitHub App connection selected for this repository;
- GitHub App installation id;
- synchronization policy and current status;
- last successful fetch/materialization information.

An administrator may add multiple GitHub App connections. Each connection stores its encrypted
private key server-side, and each repository selects one connection. The API obtains short-lived
installation tokens server-side. The browser receives neither an App private key nor an installation
token.

The minimum intended GitHub permissions are repository metadata, contents read/write, pull requests
read/write, and webhook delivery. The deployment must allowlist repositories rather than accepting
arbitrary repository names from a request. The administrator wizard discovers repositories and
branches available through the selected App connection, then stores only the selected repository
connection and branch policy.

## Shared API resources

The collaboration API is designed around resource-shaped JSON so the CLI, TUI, frontend, and CI can use the same operations. The current Docker service implements authenticated users, project listing and creation, connection discovery and setup, branch inspection, repository inspection, search, synchronization, and conflict resolution. The following list is the target resource shape; routes not described in the current deployment guide remain planned or are being materialized behind the same project boundary:

- `GET /api/me` - current user and roles;
- `GET /api/projects` - connected project list and synchronization posture;
- `GET /api/projects/{project}/packet` - current portable review packet and JSON summary;
- `GET /api/projects/{project}/search` - expectations, workflows, findings, records, source locations, and review threads;
- `GET /api/projects/{project}/timeline` - ordered human, agent, CI, Git, review, and synchronization events;
- `GET /api/projects/{project}/threads` - threaded review records with target and evidence links;
- `POST /api/projects/{project}/threads` - create a discussion or question;
- `POST /api/projects/{project}/threads/{thread}/replies` - add a reply;
- `POST /api/projects/{project}/threads/{thread}/actions` - record assignment or resolve, reopen, accept, or reject lifecycle actions according to role and policy;
- `GET /api/projects/{project}/readiness` - the same readiness buckets and next actions as the CLI;
- `GET /api/projects/{project}/findings` - deterministic findings, including compliance-relevant trust-boundary signals;
- `GET /api/projects/{project}/source/{file}` - source context only for an allowlisted connected repository;
- `GET /api/projects/{project}/sync` - active branch, pull request, queue, conflict, and last-error status.
- `GET /api/projects/{project}/sync/conflict` - structured base-branch and active-PR records for a guided conflict review.
- `POST /api/projects/{project}/sync/conflict` - submit explicit choices for same-record conflicts and materialize the result in the active PR.

The authenticated portal uses the semantic thread endpoints for review conversations. `POST /threads` creates a root discussion, `POST /threads/{thread}/replies` creates an anchored reply, and `POST /threads/{thread}/actions` records assignment or lifecycle changes. These endpoints validate the portable review record, record the authenticated audit event, and delegate materialization to the same repository-scoped synchronization worker used by the generic `POST /sync` path. A review record may carry an explicit portable anchor such as `expectation:e_123`, `verification:v_123`, `work:w_123`, or `source:src/app.rs#42`. Replies inherit their parent anchor and point to their parent review id. This lets the CLI, TUI, exported portal, and live portal show the same conversation without treating discussion as proof.

The searchable record index is refreshed from each configured base branch during authenticated inspection and by the API's periodic refresh loop. Successful Susumu synchronization also upserts the changed sidecars immediately, including removal of records that no longer exist in a changed file. This keeps search current without sending raw sidecar content to the browser. Synchronization state persists the base SHA, active branch head SHA, and observed base SHA. When the base advances, Susumu marks the repository lifecycle as requiring rebase and offers an explicit PR update action. GitHub remains the merge-conflict authority; a failed update stays visible for resolution rather than being overwritten.

### Guided conflict resolution

When the base branch advances and the active Susumu pull request cannot be updated cleanly, the portal opens a structured review rather than asking a business user to edit raw files. Susumu compares records on the base branch with records on the active PR and shows their titles and details.

- Records that exist on only one side are included automatically, so keeping both sides is the normal result when they represent different records.
- Identical records are kept without a choice.
- When the same record ID has different content on both sides, the user must explicitly keep the base version or the active PR version. Susumu does not silently overwrite either version, and it does not create duplicate IDs.
- The base branch is never written directly. The server creates the selected sidecar content and sends it through the repository's existing active PR synchronization path.

This policy keeps the common case easy while making potentially destructive decisions visible. The API accepts record IDs and choices, not arbitrary browser-supplied file content. The pull request remains the review boundary, and the resulting sidecars can still be reviewed, tested, and merged through the repository's normal controls.

All mutating requests are authenticated, scoped to a configured project, validated against the current packet, and recorded as append-only events before materialization. The API must reject unknown targets, arbitrary filesystem paths, client-supplied actor identity, client-supplied provenance, and client-supplied timestamps.

## Review event model

The live service stores actions as immutable events and derives current thread state from them. An event contains:

- event id and server timestamp;
- authenticated actor id and role;
- project and target identity;
- action type;
- human-authored title/detail when applicable;
- provenance (`human`, `automation`, `ci`, or imported provider event);
- request correlation/idempotency key;
- resulting synchronization state.

The materialized `.susu` review record remains portable. Backend-only identity and audit fields may be exported through a companion machine-readable timeline without changing a human-authored record into scanner evidence. Timestamps and provenance must be server-derived or provider-authenticated, never copied from an untrusted browser request.

Approvals, objections, unresolved disagreements, owners, questions, comments, and lifecycle changes remain review context. They do not create passed verifications, accepted compliance controls, or scanner findings.

## Pull-request synchronization

For each connected repository, Susumu maintains at most one active synchronization branch and pull request. This is never a system-wide singleton: queue, lock, branch, conflict, error, and merge state are scoped to that repository. A configured base branch selects the current lifecycle context; registering multiple allowed branches does not create multiple simultaneous active PRs for the same repository.

```text
event accepted
  -> event committed to Postgres
  -> materializer rebuilds review sidecar
  -> sync worker acquires repository lock
  -> branch created or updated
  -> .susu changes committed
  -> existing PR updated
```

- Updates are coalesced so a short conversation does not create one commit per keystroke.
- If the active PR is merged, the sync cycle is closed. The next materialized change creates a new branch and PR.
- If the active PR is closed without merging, the deployment policy decides whether to reopen it or start a replacement; the state is visible either way.
- If the branch conflicts or GitHub rejects a push, the API records a synchronization error and exposes a recovery action. It does not silently overwrite repository changes.
- Base advancement is detected by the periodic lifecycle refresh and before a queued write. The active PR can be explicitly updated with its expected head SHA. Structured sidecars reduce accidental overlap but cannot guarantee conflict-free merges; same-record edits and adjacent-line edits can still conflict and must remain visible for human resolution.

The provider boundary has a deterministic local scenario for working through this lifecycle without a real GitHub repository:

```text
cargo test --locked --features server local_github_update_branch_scenario -- --nocapture
```

The scenario runs an in-process GitHub-compatible HTTP server. One case accepts the expected PR head SHA and advances the simulated PR head; the other returns HTTP 422 and verifies that the conflict is preserved. This tests the provider update contract while keeping real GitHub integration testing separate.
- A no-diff materialization does not create an empty commit or misleading PR update.
- CI remains authoritative for rebuilding `project.susu`, `review.susu`, `check.json`, and the static portal after the PR changes land. The API may show the pending review state before CI catches up, but must label it as pending synchronization.

## Security boundary

- Browser requests use secure, HTTP-only, same-site sessions or equivalent short-lived API credentials.
- Mutating endpoints require CSRF protection when cookie sessions are used, request-size limits, rate limits, and structured validation.
- PostgreSQL credentials and GitHub App secrets stay server-side.
- Repository paths are resolved only from configured repository connections; requests cannot select arbitrary local paths.
- Webhook requests require provider signature verification before changing synchronization state.
- Audit events are append-only from the application perspective; administrative corrections create new events.
- Static exported HTML remains read-only and contains no credentials, write endpoint, or mutation controls.

## Compliance and evidence boundary

The backend must preserve the existing verification-integrity vocabulary:

- a user identity authenticates who performed an API action, not that the underlying claim is true;
- a pull request authenticates a proposed repository change, not compliance;
- a merge records repository history, not verification of a control;
- a review approval records human judgment under a process, not legal sufficiency;
- a signed commit authenticates commit identity/integrity, not test execution;
- an attestation remains declared until a configured verifier authenticates its trust policy;
- changed source, expectations, work, verification bases, and decision bases continue to produce visible dirty/stale review signals.

The frontend must use the words `declared`, `content-bound`, `human-reviewed`, `authenticated`, `pending`, `stale`, and `needs review` precisely. It must not display `certified`, `compliant`, `control met`, or `audit approved` as conclusions supplied by Susumu.

## Frontend direction

The live frontend uses a shared Susumu design direction rather than duplicating the static portal shell. The initial visual direction is warm ivory canvas, near-black ink, soft gray secondary text, restrained borders, generous whitespace, muted coral human-action accents, and calm sage/blue evidence states. The current no-build frontend provides authenticated login restoration, repository switching, multiple GitHub App connections, branch selection, synchronization posture, administrator repository registration, repository-scoped evidence inspection, structured record authoring, bounded sidecar submission, API-backed record-level fuzzy search, record detail views, anchored threads, replies, ownership, and guided conflict resolution. Broader cross-project search remains incremental.

The frontend must make the same evidence visible to business users and engineers: repository switcher, global search, readiness, findings, source context, expectations, work, verifications, decisions, review threads, ownership, timeline, synchronization status, and compliance posture. Structured authoring forms may create expectations, verifications, work records, and review comments, but the UI must keep the `.susu` syntax hidden and send only bounded, configured sidecar changes through the repository synchronization lifecycle. TUI and CLI remain available for engineers, and CI continues to use CLI and JSON surfaces.
