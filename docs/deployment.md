# Susumu server deployment

The live collaboration service is an optional feature of the Rust project. The existing CLI, TUI, static portal, and CI workflows do not require it.

## Local testing with Docker Compose

The same Compose deployment can be used to test the complete live stack locally before it is
deployed. It runs PostgreSQL, the Rust API, and the Caddy-served frontend together. Docker Engine
with Compose is supported on Linux, macOS, and Windows. GitHub configuration is optional while
testing the login, portal, search, and database behavior.

Create a local `.env` file next to `docker-compose.yml`, or copy the safe template.

On Linux or macOS:

```sh
cp .env.example .env
```

On Windows PowerShell:

```powershell
Copy-Item .env.example .env
```

Then replace the placeholder passwords. The `.env` file is ignored by Git; `.env.example` is
safe to commit because it contains no credentials.

`SUSUMU_ADMIN_PASSWORD` must contain at least 12 characters. Compose waits for the API health
check before starting the frontend, so an invalid API configuration will be visible as an API
startup error instead of producing a misleading frontend DNS error.

The minimum local settings are:

```dotenv
POSTGRES_PASSWORD=replace-with-a-long-database-password
SUSUMU_ADMIN_EMAIL=admin@example.com
SUSUMU_ADMIN_PASSWORD=replace-with-a-long-admin-password
SUSUMU_COOKIE_SECURE=false
SUSUMU_CREDENTIAL_KEY=replace-with-base64-32-byte-deployment-key
```

Use `SUSUMU_COOKIE_SECURE=false` only for local HTTP development. Production should terminate HTTPS at the reverse proxy and leave it enabled.

Start the complete local stack:

```sh
docker compose up --build
```

PowerShell users can run the same Compose command. Check the API from a Unix shell with:

```sh
curl http://localhost:8080/healthz
```

Or from PowerShell with:

```powershell
Invoke-RestMethod http://localhost:8080/healthz
```

Open `http://localhost:3000` and sign in with the administrator email and password from `.env`.
The local wizard can then add GitHub connections and repositories without editing `.env` again.
When finished, stop the local stack with `docker compose down`. The database volume remains so
local state survives a container restart; remove it only when you intentionally want a fresh test
environment.

This local workflow is also the recommended smoke test for a deployment change: start the stack,
check `/healthz`, sign in, inspect the workspace, and exercise repository setup or search before
promoting the same Compose configuration to a server.

The current server deployment exposes:

- the same-origin authenticated frontend at `http://localhost:3000` when the Compose frontend
  service is enabled; it provides login, repository switching, synchronization posture, admin
  repository registration, and bounded `.susu` change submission;

- `GET /healthz` for API/database health;
- `POST /api/auth/login` for local user sessions;
- `POST /api/auth/logout` to revoke the current session;
- `GET /api/me` for the authenticated user and roles;
- `GET /api/projects` for authenticated users to list active repository connections;
- `GET /api/github/connections` for administrators to list active GitHub App connections without
  returning private keys;
- `GET /api/github/repositories?connection_id=...` for administrators to discover repositories
  available through a selected GitHub App connection;
- `GET /api/github/branches?connection_id=...&owner=...&repository=...` for administrators to
  load the existing branches of a selected repository;
- `POST /api/github/setup` for administrators to add a named GitHub App connection. The API
  encrypts the key before storing it in PostgreSQL and updates the running provider without a
  restart;
- `POST /api/projects` for administrators to register an allowlisted GitHub repository and
  its base branches;
- `POST /api/projects/{project_key}/sync` for authenticated users to queue one configured
  repository/base-branch synchronization request. It may include bounded UTF-8 `.susu` file
  changes; when GitHub is configured, those changes are materialized through the server-side
  worker and create or update that repository's one active pull request.
- `PUT /api/projects/{project_key}/sync` to explicitly update a conflicted active pull request
  against the observed base branch. If GitHub cannot apply the update, the conflict remains
  visible and must be resolved before retrying.
- `POST /api/projects/{project_key}/github/validate` for administrators to validate the
  configured GitHub App installation without returning an installation token.
- `POST /api/projects/{project_key}/github/inspect` for authenticated users to inspect the configured
  base branch and receive its current Git SHA.
- `GET /api/projects/{project_key}/search` for authenticated, branch-scoped ranked fuzzy search
  over indexed record summaries. The API refreshes the index during inspection, after successful
  sidecar synchronization, and periodically for externally merged repository changes.

Login issues a secure session cookie and a readable CSRF cookie. Browser clients must echo the
CSRF cookie in the `X-Susumu-CSRF` header for repository registration and sync requests.

Repository registration stores the GitHub App connection id, installation id, selected base branch,
and the complete supported Susumu sidecar set. It creates one idle sync state for every configured
repository/base-branch pair. The provider boundary performs server-side branch, file, and pull-request
operations for the synchronization worker; it does not expose private keys, installation tokens,
arbitrary local paths, or raw provider write operations to the browser.

The bootstrap administrator is created only when both `SUSUMU_ADMIN_EMAIL` and `SUSUMU_ADMIN_PASSWORD` are supplied, and an existing email is never overwritten on restart. Passwords are stored as Argon2 hashes. Session tokens are random opaque values; PostgreSQL stores only their SHA-256 hashes.

GitHub App configuration is optional during local development. The recommended setup is to set
`SUSUMU_CREDENTIAL_KEY` once for the deployment, then use the administrator onboarding wizard to
add a named GitHub App connection with its App ID and PEM private key. The API encrypts the PEM
with AES-256-GCM before storing it in PostgreSQL and loads the connection into the running process,
so the administrator does not need to edit `.env` or restart the containers. The browser never
receives the stored key. Additional GitHub App connections can be added later and selected when
attaching repositories.
`SUSUMU_GITHUB_APP_ID` and `SUSUMU_GITHUB_APP_PRIVATE_KEY_FILE` remain supported for deployments
that prefer a mounted server-side PEM file. Do not provide both setup methods for the same app.

## Production boundary

Deploy the Compose stack on a host that runs Docker Engine and Compose, or on a managed container
platform that provides the same services and persistent PostgreSQL storage. The deployment owner
is responsible for the domain, HTTPS termination, secret storage, database backups, GitHub App
installation, and access policy. The application is not limited to a particular operating system
or shell.

Place the API behind HTTPS and a reverse proxy that provides normal operational protections such as access logging, request limits, and controlled network exposure. Keep PostgreSQL private to the Compose network. Store `POSTGRES_PASSWORD`, `SUSUMU_ADMIN_PASSWORD`, and future GitHub App credentials in a secret manager or deployment secret store.

The API image and same-origin frontend run behind the same Compose deployment boundary. Caddy serves
the frontend, proxies `/api` to Rust, and can manage production HTTPS when `SUSUMU_FRONTEND_HOST`
is set to a real domain; its certificate/configuration volumes must be retained. The frontend
covers authenticated repository registration, multiple GitHub connections, repository switching,
branch selection, synchronization posture, repository evidence inspection, API-backed fuzzy search,
structured authoring, record detail views, anchored review threads, and guided conflict resolution.
Static HTML remains read-only. Queue-only requests remain available for CI and CLI workflows.
Webhooks and broader resource-shaped collaboration endpoints remain future work.

## Rust layout

The server code is feature-gated and separated from the local product path:

```text
src/bin/susumu-server.rs   # server binary entry point
src/server/mod.rs          # application wiring and routes
src/server/config.rs       # environment configuration
src/server/db.rs           # pool, migrations, users, sessions, projects, sync state
src/server/auth.rs         # login, logout, current-user session
src/server/repository.rs   # validated repository registration and listing
src/server/sync.rs         # authenticated sync queue transitions
src/server/worker.rs      # per-repository materialization and one-active-PR synchronization
src/server/error.rs        # safe API error responses
migrations/                 # versioned PostgreSQL migrations
frontend/index.html        # authenticated business and engineering workspace shell
frontend/app.js            # same-origin API client and current workflow interactions
frontend/styles.css        # Susumu workspace visual system
frontend/Caddyfile         # static frontend plus same-origin API reverse proxy
Dockerfile                  # reproducible server image
docker-compose.yml          # frontend + API + PostgreSQL local deployment
```
