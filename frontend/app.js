const state = { user: null, projects: [], selected: null, onboardingRepositories: [], githubConnections: [], repositoryFiles: [] };

const $ = (selector) => document.querySelector(selector);
const loginView = $("#login-view");
const appView = $("#app-view");

function csrfToken() {
  return document.cookie.split(";").map((part) => part.trim()).find((part) => part.startsWith("susumu_csrf="))?.split("=")[1] ?? "";
}

async function api(path, options = {}) {
  const method = options.method ?? "GET";
  const headers = { Accept: "application/json", ...(options.body ? { "Content-Type": "application/json" } : {}), ...(options.headers ?? {}) };
  if (!["GET", "HEAD"].includes(method)) headers["X-Susumu-CSRF"] = csrfToken();
  const response = await fetch(path, { credentials: "same-origin", cache: "no-store", ...options, headers });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error ?? `Request failed (${response.status})`);
  return body;
}

function showNotice(message, error = false) {
  const notice = $("#notice");
  notice.textContent = message;
  notice.hidden = false;
  notice.classList.toggle("error", error);
}

function setSignedIn(user) {
  state.user = user;
  $("#session-loading").hidden = true;
  loginView.hidden = true;
  appView.hidden = false;
  $("#identity").textContent = user.display_name || user.email;
  $("#identity").hidden = false;
  $("#logout").hidden = false;
  $("#new-project").hidden = !user.roles.includes("admin");
}

function setSignedOut() {
  state.user = null;
  $("#session-loading").hidden = true;
  state.projects = [];
  state.selected = null;
  loginView.hidden = false;
  appView.hidden = true;
  $("#identity").hidden = true;
  $("#logout").hidden = true;
}

function onboardingSkipped() {
  return sessionStorage.getItem("susumu_onboarding_skipped") === "true";
}

function renderEmptyWorkspace() {
  $("#project-detail").innerHTML = `<div class="card empty-state"><span class="empty-glyph" aria-hidden="true"><svg class="icon" viewBox="0 0 24 24"><path d="M12 5v14M5 12h14" /></svg></span><h2>Connect a repository</h2><p class="muted">An administrator can add the first project from the plus button.</p><button id="start-onboarding" class="button button-quiet" type="button">Start setup</button></div>`;
  $("#start-onboarding").addEventListener("click", () => {
    sessionStorage.removeItem("susumu_onboarding_skipped");
    renderOnboarding();
  });
}

function renderOnboarding() {
  $("#project-detail").innerHTML = `<section class="card onboarding-card" aria-labelledby="onboarding-title"><div class="onboarding-intro"><p class="eyebrow">First connection</p><h2 id="onboarding-title">Bring a repository into Susumu.</h2><p class="muted">Connect the GitHub App once, choose a repository, and Susumu will keep its project memory in the repository’s review lifecycle.</p></div><div class="setup-steps"><div class="setup-step active" data-step="1"><span class="step-number">1</span><div><strong>Connect GitHub</strong><span>Use the server-side GitHub App.</span></div></div><div class="setup-step" data-step="2"><span class="step-number">2</span><div><strong>Choose a repository</strong><span>Only repositories installed for Susumu appear.</span></div></div><div class="setup-step" data-step="3"><span class="step-number">3</span><div><strong>Set project defaults</strong><span>Choose the branch and sidecars Susumu may update.</span></div></div></div><div id="onboarding-panel" class="onboarding-panel"><p class="muted">Checking the server connection...</p></div><button id="skip-onboarding" class="button button-quiet onboarding-skip" type="button">Skip for now</button></section>`;
  $("#skip-onboarding").addEventListener("click", () => {
    sessionStorage.setItem("susumu_onboarding_skipped", "true");
    renderEmptyWorkspace();
  });
  checkGithubSetup();
}

function setOnboardingStep(step) {
  $(".setup-steps").querySelectorAll(".setup-step").forEach((item) => {
    item.classList.toggle("active", Number(item.dataset.step) === step);
    item.classList.toggle("complete", Number(item.dataset.step) < step);
  });
}

function setOnboardingPanel(content) {
  const panel = $("#onboarding-panel");
  panel.innerHTML = `<div class="setup-guide"><div class="setup-guide-heading"><strong>What is the GitHub App?</strong><button id="github-app-info" class="info-button" type="button" aria-expanded="false" aria-controls="github-app-info-panel" title="About GitHub App setup">i</button></div><div id="github-app-info-panel" class="setup-guide-panel" hidden><p>Susumu uses one private GitHub App for this deployment. Create it in GitHub Settings, generate its private key, then install it on the organization or repositories Susumu may update.</p><p class="setup-guide-label">Required repository permissions</p><ul><li>Contents: Read and write</li><li>Pull requests: Read and write</li><li>Metadata: Read-only</li></ul><p>The App ID and private key are sent only to the authenticated Susumu API. The key is encrypted before storage and is never returned to the browser.</p></div></div>${content}`;
  const info = $("#github-app-info");
  const details = $("#github-app-info-panel");
  info.addEventListener("click", () => {
    const expanded = info.getAttribute("aria-expanded") === "true";
    info.setAttribute("aria-expanded", String(!expanded));
    details.hidden = expanded;
  });
}

async function checkGithubSetup() {
  const panel = $("#onboarding-panel");
  try {
    const health = await api("/healthz");
    if (!health.credential_encryption) {
      setOnboardingPanel(`<div class="setup-message setup-message-warning"><strong>One deployment setting is needed first.</strong><p>The deployment operator must provide the credential encryption key once. You will not need to edit environment files for this setup.</p></div>`);
      return;
    }
    if (!health.github_app) {
      setOnboardingPanel(`<div class="setup-message"><strong>Connect the GitHub App.</strong><p>Paste the App ID and PEM private key from the GitHub App settings. Susumu encrypts the key before it reaches the database and never sends it back to the browser.</p><form id="github-setup-form" class="setup-form"><label>GitHub App ID<input name="app_id" inputmode="numeric" pattern="[0-9]+" required /></label><label>Private key PEM<textarea name="private_key_pem" rows="7" spellcheck="false" required placeholder="-----BEGIN PRIVATE KEY-----"></textarea></label><p id="github-setup-error" class="form-error" role="alert" hidden></p><button class="button button-primary" type="submit">Secure GitHub connection</button></form></div>`);
      $("#github-setup-form").addEventListener("submit", submitGithubSetup);
      return;
    }
    await loadOnboardingRepositories();
  } catch (error) {
    panel.innerHTML = `<div class="setup-message setup-message-warning"><strong>We could not check the server connection.</strong><p>${escapeHtml(error.message)}</p></div>`;
  }
}

async function submitGithubSetup(event) {
  event.preventDefault();
  const setupForm = event.currentTarget;
  const form = new FormData(setupForm);
  const error = $("#github-setup-error");
  error.hidden = true;
  try {
    await api("/api/github/setup", { method: "POST", body: JSON.stringify({ app_id: Number(form.get("app_id")), private_key_pem: form.get("private_key_pem") }) });
    setupForm.reset();
    await loadOnboardingRepositories();
  } catch (setupError) {
    error.textContent = setupError.message;
    error.hidden = false;
  }
}

async function loadOnboardingRepositories() {
  const panel = $("#onboarding-panel");
  setOnboardingStep(2);
  panel.innerHTML = '<p class="muted">Finding repositories installed for Susumu...</p>';
  try {
    state.githubConnections = await api("/api/github/connections");
    const connectionId = state.githubConnections[0]?.id;
    state.onboardingRepositories = await api(connectionId ? "/api/github/repositories?connection_id=" + encodeURIComponent(connectionId) : "/api/github/repositories");
    if (!state.onboardingRepositories.length) {
      panel.innerHTML = '<div class="setup-message setup-message-warning"><strong>No repositories are available yet.</strong><p>Install the GitHub App on a repository or organization, then return here and try again.</p><button id="refresh-repositories" class="button button-quiet" type="button">Check again</button></div>';
      $("#refresh-repositories").addEventListener("click", loadOnboardingRepositories);
      return;
    }
    panel.innerHTML = `<div class="setup-message"><strong>Choose the first repository.</strong><p>Susumu will only be able to update the repository you connect here.</p><div class="repository-picker">${state.onboardingRepositories.map((repository, index) => `<button class="repository-option" type="button" data-repository-index="${index}"><strong>${escapeHtml(repository.full_name)}</strong><span>${repository.private ? "Private" : "Public"} · default branch ${escapeHtml(repository.default_branch || "not reported")}</span></button>`).join("")}</div><p id="repository-setup-error" class="form-error" role="alert" hidden></p></div>`;
    panel.querySelectorAll("[data-repository-index]").forEach((button) => button.addEventListener("click", () => renderRepositoryDefaults(state.onboardingRepositories[Number(button.dataset.repositoryIndex)])));
  } catch (error) {
    panel.innerHTML = `<div class="setup-message setup-message-warning"><strong>Repository discovery needs attention.</strong><p>${escapeHtml(error.message)}</p><button id="refresh-repositories" class="button button-quiet" type="button">Try again</button></div>`;
    $("#refresh-repositories").addEventListener("click", loadOnboardingRepositories);
  }
}

function renderRepositoryDefaults(repository) {
  setOnboardingStep(3);
  const projectKey = repository.full_name.replace(/[^A-Za-z0-9._-]+/g, "-").toLowerCase();
  $("#onboarding-panel").innerHTML = `<div class="setup-message"><strong>Set the project defaults.</strong><p>Choose the repository branch Susumu should use. All supported Susumu record files remain available automatically.</p><form id="repository-setup-form" class="setup-form"><label>Project key<input name="project_key" value="${escapeHtml(projectKey)}" pattern="[A-Za-z0-9._-]+" required /></label><label>Display name<input name="display_name" value="${escapeHtml(repository.full_name)}" required /></label><label>Base branch<select name="allowed_base_branches" required disabled><option value="">Finding branches...</option></select></label><p id="repository-setup-error" class="form-error" role="alert" hidden></p><div class="dialog-actions"><button id="back-to-repositories" class="button button-quiet" type="button">Back</button><button class="button button-primary" type="submit">Connect repository</button></div></form></div>`;
  loadRepositoryBranches(repository).then((branches) => {
    const select = $("#repository-setup-form").elements.allowed_base_branches;
    select.innerHTML = branches.map((branch) => `<option value="${escapeHtml(branch)}">${escapeHtml(branch)}</option>`).join("");
    select.value = repository.default_branch || branches[0] || "";
    select.disabled = !branches.length;
  }).catch((error) => { $("#repository-setup-error").textContent = error.message; $("#repository-setup-error").hidden = false; });
  $("#back-to-repositories").addEventListener("click", loadOnboardingRepositories);
  $("#repository-setup-form").addEventListener("submit", (event) => submitRepositorySetup(event, repository));
}

async function submitRepositorySetup(event, repository) {
  event.preventDefault();
  const form = new FormData(event.currentTarget);
  const error = $("#repository-setup-error");
  error.hidden = true;
  try {
    await api("/api/projects", { method: "POST", body: JSON.stringify({ project_key: form.get("project_key"), display_name: form.get("display_name"), provider: "github", repository_owner: repository.repository_owner, repository_name: repository.repository_name, installation_id: repository.installation_id, github_connection_id: repository.github_connection_id || undefined, allowed_base_branches: [form.get("allowed_base_branches")] }) });
    sessionStorage.removeItem("susumu_onboarding_skipped");
    await loadProjects();
    showNotice("Repository connected. Its synchronization states are ready.");
  } catch (setupError) {
    error.textContent = setupError.message;
    error.hidden = false;
  }
}

function renderProjects() {
  const list = $("#project-list");
  if (!state.projects.length) {
    list.innerHTML = '<p class="muted">No repositories are connected yet.</p>';
    if (state.user?.roles.includes("admin") && !onboardingSkipped()) renderOnboarding();
    else renderEmptyWorkspace();
    return;
  }
  list.innerHTML = state.projects.map((project) => { const repositoryLabel = `${project.repository_owner}/${project.repository_name}`; const secondaryLabel = project.display_name.trim().toLowerCase() === repositoryLabel.toLowerCase() ? "GitHub repository" : repositoryLabel; return `<button class="project-button ${state.selected?.project_key === project.project_key ? "active" : ""}" data-project="${escapeHtml(project.project_key)}"><strong>${escapeHtml(project.display_name)}</strong><span>${escapeHtml(secondaryLabel)}</span></button>`; }).join("");
  list.querySelectorAll("[data-project]").forEach((button) => button.addEventListener("click", () => selectProject(button.dataset.project)));
  if (!state.selected) selectProject(state.projects[0].project_key);
}

function selectProject(projectKey) {
  state.selected = state.projects.find((project) => project.project_key === projectKey) ?? null;
  state.repositoryFiles = [];
  searchSequence += 1;
  renderProjects();
  renderDetail();
}

function renderDetail() {
  const project = state.selected;
  if (!project) return;
  const conflict = project.sync.find((sync) => sync.rebase_required);
  const syncRows = project.sync.map((sync) => `<div class="sync-row"><div><div class="sync-branch">${escapeHtml(sync.base_branch)}</div><div class="sync-meta">${sync.active_branch ? `branch ${escapeHtml(sync.active_branch)}` : "No active synchronization branch"}${sync.pull_request_number ? ` · PR #${sync.pull_request_number}` : ""}</div></div><span class="pill ${escapeHtml(sync.status)}">${escapeHtml(sync.status)}</span><span></span>${sync.last_error ? `<div class="sync-meta">${escapeHtml(sync.last_error)}</div>` : ""}</div>`).join("");
  $("#project-detail").innerHTML = `<section class="card project-header"><p class="eyebrow">Connected repository</p><h2>${escapeHtml(project.display_name)}</h2><p class="repo-line"><code>${escapeHtml(project.repository_owner)}/${escapeHtml(project.repository_name)}</code> · ${escapeHtml(project.provider)}</p><div class="sync-list">${syncRows}</div></section><section class="card action-card authoring-card ${conflict ? "authoring-locked" : ""}"><div class="action-card-header"><div><h3>Add to project memory</h3>${conflict ? '<p class="conflict-lock-copy">New entries are paused until the repository conflict is resolved.</p>' : ""}</div><span class="pill ${conflict ? "conflict" : "pending"} entry-pill">${conflict ? "Resolve conflict first" : '<span class="entry-long">Structured entry</span><span class="entry-short">Entry</span>'}</span></div><div class="authoring-tabs" role="tablist" aria-label="Add a project memory record">${["expectation", "verification", "work", "review"].map((kind) => `<button class="authoring-tab" type="button" data-authoring-kind="${kind}" role="tab" aria-selected="${kind === "expectation" && !conflict}" ${conflict ? "disabled" : ""}>${kind === "review" ? "Review comment" : kind[0].toUpperCase() + kind.slice(1)}</button>`).join("")}</div><div id="authoring-panel"></div></section>`;
  if (conflict) {
    const notice = document.createElement("div");
    notice.className = "sync-conflict-notice";
    notice.innerHTML = `<strong>The base branch has advanced.</strong><span>${escapeHtml(conflict.conflict_detail || "The active pull request needs an update before more changes can be sent.")}</span><span class="sync-conflict-help">Susumu will not overwrite either side. Review the changed records here, choose what should remain, and send the result to the active pull request.</span><p class="sync-conflict-result" role="alert" hidden></p><div class="sync-conflict-actions"><button class="button button-primary" type="button" data-open-conflict>Review and resolve</button><button class="button button-quiet" type="button" data-safe-rebase>Try automatic update</button></div>`;
    notice.querySelector("[data-safe-rebase]").addEventListener("click", (event) => retryRebase(event.currentTarget));
    notice.querySelector("[data-open-conflict]").addEventListener("click", () => loadConflictResolver(project));
    $("#project-detail").querySelector(".project-header").append(notice);
  }
  document.querySelectorAll(".authoring-tab").forEach((button) => button.addEventListener("click", () => renderAuthoringPanel(button.dataset.authoringKind)));
  renderAuthoringPanel("expectation");
  const evidenceCard = document.createElement("section");
  evidenceCard.className = "card evidence-card";
  evidenceCard.innerHTML = "<div class=\"action-card-header\"><div><h3>Project memory</h3><p>Susumu scans the connected repository and presents records by purpose. Files are read from the selected base branch.</p></div></div><div id=\"repository-evidence\"><p class=\"muted\">Scanning configured Susumu records...</p></div>";
  const nextWorkCard = document.createElement("section");
  nextWorkCard.className = "card next-work-card";
  nextWorkCard.innerHTML = `<div class="next-work-content"><p class="eyebrow">Next work</p><h3>What still needs attention?</h3><p class="muted">Scanning the project memory for the next useful action...</p></div><button id="open-next-work" class="button button-quiet" type="button" ${conflict ? "disabled" : ""}>${conflict ? "Resolve conflict first" : "Open Work entry"}</button>`;
  $("#project-detail").append(nextWorkCard);
  if (!conflict) $("#open-next-work").addEventListener("click", () => { renderAuthoringPanel("work"); $("#authoring-panel").scrollIntoView({ behavior: "smooth", block: "start" }); });
  $("#project-detail").append(evidenceCard);
  loadRepositoryEvidence(project);
}

async function retryRebase(button) {
  button.disabled = true;
  const result = button.closest(".sync-conflict-notice")?.querySelector(".sync-conflict-result");
  if (result) result.hidden = true;
  try {
    await api(`/api/projects/${encodeURIComponent(state.selected.project_key)}/sync`, { method: "PUT" });
    showNotice("The active pull request was updated to the current base branch.");
    await loadProjects();
  } catch (error) {
    if (result) {
      result.textContent = `The conflict still needs resolution: ${error.message}`;
      result.hidden = false;
    }
    showNotice("The conflict is still unresolved.", true);
    button.disabled = false;
  }
}

async function loadConflictResolver(project) {
  const existing = $("#conflict-resolver");
  if (existing) existing.remove();
  const resolver = document.createElement("section");
  resolver.id = "conflict-resolver";
  resolver.className = "card conflict-resolver";
  resolver.innerHTML = '<p class="muted">Reading the two project-memory versions...</p>';
  $("#project-detail").append(resolver);
  resolver.setAttribute("tabindex", "-1");
  requestAnimationFrame(() => resolver.scrollIntoView({ behavior: "smooth", block: "start" }));
  try {
    const conflict = await api(`/api/projects/${encodeURIComponent(project.project_key)}/sync/conflict`);
    renderConflictResolver(project, conflict);
  } catch (error) {
    resolver.innerHTML = `<strong>We could not open the conflict review.</strong><p class="form-error">${escapeHtml(error.message)}</p>`;
  }
}

function renderConflictResolver(project, conflict) {
  const resolver = $("#conflict-resolver");
  const choices = new Map();
  const changedFiles = conflict.files.filter((file) => file.records.some((record) => record.changed));
  resolver.innerHTML = `<div class="conflict-resolver-heading"><div><p class="eyebrow">Guided conflict review</p><h3>Choose what belongs in project memory</h3><p class="muted">The base branch is the current repository truth. The active PR contains Susumu work. Records that exist on only one side are kept automatically. For the same record on both sides, choose one version.</p></div></div><div class="conflict-resolver-files">${changedFiles.length ? changedFiles.map((file) => conflictFileMarkup(file)).join("") : '<p class="muted">There are no record-level differences to review.</p>'}</div><div class="conflict-resolver-footer"><p class="conflict-resolver-note"><strong>Keep both is the default when records are different.</strong> Susumu combines records unique to the base branch with records unique to the active PR. If the same record changed on both sides, choose the version to keep. The base branch will not be changed directly.</p><p id="conflict-resolver-error" class="form-error" role="alert" hidden></p><div class="dialog-actions"><button class="button button-quiet" type="button" data-cancel-conflict>Cancel</button><button class="button button-primary" type="button" data-submit-conflict>Resolve</button></div></div>`;
  requestAnimationFrame(() => {
    resolver.scrollIntoView({ behavior: "smooth", block: "start" });
    resolver.focus({ preventScroll: true });
  });
  resolver.querySelectorAll("[data-conflict-choice]").forEach((button) => button.addEventListener("click", () => {
    const key = `${button.dataset.path}:${button.dataset.recordId}`;
    choices.set(key, button.dataset.conflictChoice);
    button.closest(".conflict-choice-group").querySelectorAll("[data-conflict-choice]").forEach((candidate) => candidate.classList.toggle("selected", candidate === button));
  }));
  resolver.querySelector("[data-cancel-conflict]").addEventListener("click", () => resolver.remove());
  resolver.querySelector("[data-submit-conflict]").addEventListener("click", async (event) => {
    const error = resolver.querySelector("#conflict-resolver-error");
    error.hidden = true;
    const unresolved = conflict.files.flatMap((file) => file.records.filter((record) => record.changed && record.base && record.active && !choices.has(`${file.path}:${record.id}`)));
    if (unresolved.length) {
      error.textContent = `Choose a version for ${unresolved.length} record${unresolved.length === 1 ? "" : "s"} before continuing.`;
      error.hidden = false;
      return;
    }
    event.currentTarget.disabled = true;
    try {
      await api(`/api/projects/${encodeURIComponent(project.project_key)}/sync/conflict`, { method: "POST", body: JSON.stringify({ base_branch: conflict.base_branch, choices: [...choices].map(([key, choice]) => { const split = key.indexOf(":"); return { path: key.slice(0, split), record_id: key.slice(split + 1), choice }; }) }) });
      showNotice("The conflict resolution is now in the active pull request.");
      resolver.remove();
      await loadProjects();
    } catch (submitError) {
      error.textContent = submitError.message;
      error.hidden = false;
      event.currentTarget.disabled = false;
    }
  });
}

function conflictFileMarkup(file) {
  const records = file.records.filter((record) => record.changed);
  return `<section class="conflict-file"><div class="conflict-file-heading"><div><p class="eyebrow">${escapeHtml(file.path)}</p><strong>Records included from both sides</strong></div><span class="muted">Different records are kept automatically. Only matching IDs need a choice.</span></div>${records.map((record) => { const unique = !record.base || !record.active; return `<article class="conflict-record ${unique ? "conflict-record-automatic" : ""}"><div class="conflict-record-title"><strong>${escapeHtml(record.base?.title || record.active?.title || record.id)}</strong><small>${unique ? "Included automatically" : escapeHtml(record.id)}</small></div><div class="conflict-sides"><div class="conflict-side ${record.base ? "" : "missing"}"><span class="pill">Base branch</span>${record.base ? `<strong>${escapeHtml(record.base.title)}</strong><p>${escapeHtml(record.base.detail)}</p>` : "<p>This record is not on the base branch.</p>"}</div><div class="conflict-side ${record.active ? "" : "missing"}"><span class="pill pending">Active PR</span>${record.active ? `<strong>${escapeHtml(record.active.title)}</strong><p>${escapeHtml(record.active.detail)}</p>` : "<p>This record is not in the active PR.</p>"}</div></div>${record.base && record.active ? `<div class="conflict-choice-group"><span>Which version should remain?</span><div class="conflict-choice-buttons"><button class="button button-quiet" type="button" data-conflict-choice data-path="${escapeHtml(file.path)}" data-record-id="${escapeHtml(record.id)}" data-conflict-choice="base">Keep base</button><button class="button button-quiet" type="button" data-conflict-choice data-path="${escapeHtml(file.path)}" data-record-id="${escapeHtml(record.id)}" data-conflict-choice="active">Keep active PR</button></div></div>` : "<p class=\"conflict-automatic-note\">This record is unique to one side, so it will be included without a choice.</p>"}</article>`; }).join("")}</section>`;
}

const authoringTypes = {
  expectation: { label: "Expectation", path: "expectations.susu" },
  verification: { label: "Verification", path: "verifications.susu" },
  work: { label: "Work", path: "work.susu" },
  review: { label: "Review comment", path: "review.susu" },
};

function recordId(prefix) {
  const random = globalThis.crypto?.randomUUID?.().replaceAll("-", "") ?? `${Date.now()}${Math.random()}`;
  return `${prefix}_${random.slice(0, 16)}`;
}

function recordValue(value) {
  return String(value ?? "").trim().replace(/[\r\n]+/g, " ").replace(/"/g, "'");
}

function expectationOptions() {
  const file = state.repositoryFiles.find((item) => item.path === "expectations.susu");
  const ids = (typeof file?.content === "string" ? file.content : "").matchAll(/^expectation\s+(\S+)/gm);
  return Array.from(ids, (match) => match[1]);
}

function expectationSelect(required = false) {
  const options = expectationOptions();
  return `<label>Expectation${required ? "" : " (optional)"}<select name="expectation_id" ${required ? "required" : ""}><option value="">${required ? "Choose an expectation" : "Project-level record"}</option>${options.map((id) => `<option value="${escapeHtml(id)}">${escapeHtml(id)}</option>`).join("")}</select></label>`;
}

function reviewOwners() {
  return [...new Set(state.repositoryFiles.flatMap(parseRecords).filter((record) => record.kind === "review").map((record) => record.owner).filter(Boolean).concat(state.user?.display_name ?? ""))].sort();
}

function renderAuthoringPanel(kind, context = {}) {
  const panel = $("#authoring-panel");
  if (!panel) return;
  const conflict = state.selected?.sync.some((sync) => sync.rebase_required);
  if (conflict) {
    panel.innerHTML = '<div class="authoring-locked-message"><strong>Project memory is temporarily read-only.</strong><span>Resolve the repository conflict above before adding expectations, verifications, work, or review comments.</span></div>';
    return;
  }
  document.querySelectorAll(".authoring-tab").forEach((button) => {
    const active = button.dataset.authoringKind === kind;
    button.classList.toggle("active", active);
    button.setAttribute("aria-selected", String(active));
  });
  const common = `<label>Title<input name="title" maxlength="240" required /></label><label>Details<textarea name="detail" rows="4" maxlength="2000" required></textarea></label>`;
  const fields = {
    expectation: `<div class="authoring-intro"><strong>Describe what should be true.</strong><span>Expectations are authored intent and remain distinct from scanner findings.</span></div>${common}<label>Status<select name="status"><option value="proposed">Proposed</option><option value="accepted">Accepted</option></select></label>`,
    verification: `<div class="authoring-intro"><strong>Record how an expectation was checked.</strong><span>Verification records evidence and outcome. They do not silently accept an expectation.</span></div>${expectationSelect(true)}<label>Result<select name="status"><option value="passed">Passed</option><option value="failed">Failed</option><option value="inconclusive">Inconclusive</option></select></label><label>Method<input name="method" placeholder="For example: manual review or CI test" required /></label>${common.replace('<label>Title<input name="title" maxlength="240" required /></label>', "")}`,
    work: `<div class="authoring-intro"><strong>Connect planned or completed work.</strong><span>Link the work to an expectation when this is part of a larger outcome.</span></div>${expectationSelect()}<label>Kind<select name="kind"><option value="implementation">Implementation</option><option value="maintenance">Maintenance</option><option value="infrastructure">Infrastructure</option></select></label><label>Status<select name="status"><option value="planned">Planned</option><option value="in_progress">In progress</option><option value="completed">Completed</option></select></label>${common}`,
    review: `<div class="authoring-intro"><strong>${context.parent ? "Reply to this review thread." : "Start a review conversation."}</strong><span>Discussion stays anchored to the selected record and remains separate from verification.</span></div>${context.anchorKind ? `<div class="anchor-context"><span class="eyebrow">Anchored to</span><strong>${escapeHtml(context.anchorKind)} · ${escapeHtml(context.anchorId)}</strong><input type="hidden" name="anchor_kind" value="${escapeHtml(context.anchorKind)}" /><input type="hidden" name="anchor_id" value="${escapeHtml(context.anchorId)}" />${context.parent ? `<input type="hidden" name="parent" value="${escapeHtml(context.parent)}" />` : ""}</div>` : `<label>Anchor type<select name="anchor_kind"><option value="">Project discussion</option><option value="expectation">Expectation</option><option value="verification">Verification</option><option value="work">Work</option></select></label><label>Anchor record<input name="anchor_id" placeholder="Record ID (optional)" /></label>`}<label>Owner<input name="owner" list="review-owner-options" placeholder="Person or team" required /><datalist id="review-owner-options">${reviewOwners().map((owner) => `<option value="${escapeHtml(owner)}"></option>`).join("")}</datalist></label><label>Comment type<select name="comment_type"><option value="question">Question</option><option value="comment">Comment</option><option value="objection">Objection</option><option value="approval">Approval</option><option value="risk">Risk</option></select></label><label>Status<select name="status"><option value="open">Open</option><option value="resolved">Resolved</option><option value="accepted">Accepted</option><option value="rejected">Rejected</option></select></label>${common}`,
  }[kind];
  panel.innerHTML = `<form id="authoring-form" class="authoring-form" data-authoring-kind="${escapeHtml(kind)}">${fields}<p id="authoring-error" class="form-error" role="alert" hidden></p><div class="dialog-actions"><button class="button button-primary" type="submit">Add ${escapeHtml(authoringTypes[kind].label)}</button></div></form>`;
  panel.querySelector("form").addEventListener("submit", submitAuthoringRecord);
}

function buildAuthoringRecord(kind, form) {
  const data = Object.fromEntries(new FormData(form));
  const title = recordValue(data.title);
  const detail = recordValue(data.detail);
  if (kind === "expectation") return `expectation ${recordId("e")} target=project subject=- status=${data.status} source="human:portal" title="${title}" detail="${detail}";`;
  if (kind === "verification") return `verification ${recordId("v")} expectation=${data.expectation_id} status=${data.status} method="${recordValue(data.method)}" source="human:portal" evidence=- basis=- detail="${detail}";`;
  if (kind === "work") return `work ${recordId("w")} target=project subject=- expectation=${data.expectation_id || "-"} kind=${data.kind} status=${data.status} source="human:portal" evidence=- title="${title}" detail="${detail}";`;
  const anchor = data.anchor_kind && data.anchor_id ? `${data.anchor_kind}:${recordValue(data.anchor_id)}` : "-";
  const parent = data.parent || "-";
  return `review ${recordId("r")} target=project subject=- anchor=${anchor} parent=${parent} kind=${data.comment_type || "comment"} status=${data.status} owner="${recordValue(data.owner)}" source="human:portal" title="${title}" detail="${detail}";`;
}

async function submitAuthoringRecord(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const kind = form.dataset.authoringKind;
  const path = authoringTypes[kind].path;
  const error = $("#authoring-error");
  const button = form.querySelector("button[type=submit]");
  const configured = state.selected.sidecar_paths.includes(path);
  if (!configured) { error.textContent = `${path} is not configured for this repository.`; error.hidden = false; return; }
  const existing = state.repositoryFiles.find((item) => item.path === path);
  const currentContent = typeof existing?.content === "string" ? existing.content.trimEnd() : "";
  button.disabled = true;
  error.hidden = true;
  try {
    const baseBranch = state.selected.allowed_base_branches[0];
    const data = Object.fromEntries(new FormData(form));
    let result;
    if (kind === "review") {
      const payload = {
        base_branch: baseBranch,
        anchor: data.anchor_kind && data.anchor_id ? `${data.anchor_kind}:${recordValue(data.anchor_id)}` : null,
        parent: data.parent || null,
        kind: data.comment_type || "comment",
        status: data.status || "open",
        owner: recordValue(data.owner),
        title: recordValue(data.title),
        detail: recordValue(data.detail),
      };
      const endpoint = payload.parent ? `/api/projects/${encodeURIComponent(state.selected.project_key)}/threads/${encodeURIComponent(payload.parent)}/replies` : `/api/projects/${encodeURIComponent(state.selected.project_key)}/threads`;
      result = await api(endpoint, { method: "POST", body: JSON.stringify(payload) });
    } else {
      result = await api(`/api/projects/${encodeURIComponent(state.selected.project_key)}/sync`, { method: "POST", body: JSON.stringify({ base_branch: baseBranch, changes: [{ path, content: `${currentContent}${currentContent ? "\n" : ""}${buildAuthoringRecord(kind, form)}\n` }] }) });
    }
    showNotice(result.status === "pending" ? "Your entry is in the active pull request and ready for human review." : `Synchronization is ${result.status}.`);
    await loadRepositoryEvidence(state.selected);
    form.reset();
  } catch (submitError) {
    error.textContent = submitError.message;
    error.hidden = false;
  } finally { button.disabled = false; }
}

function recordArea(path) {
  return { "expectations.susu": "Expectations", "verifications.susu": "Verifications", "work.susu": "Work", "decisions.susu": "Decisions", "review.susu": "Review" }[path] ?? path.replace(/\.susu$/, "").replace(/[-_]/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

async function loadRepositoryEvidence(project) {
  const branch = project.allowed_base_branches[0];
  try {
    const result = await api("/api/projects/" + encodeURIComponent(project.project_key) + "/github/inspect", { method: "POST", body: JSON.stringify({ base_branch: branch }) });
    if (state.selected?.project_key !== project.project_key) return;
    state.repositoryFiles = result.files ?? [];
    const present = state.repositoryFiles.filter((file) => file.present);
    $("#repository-evidence").innerHTML = present.length ? "<div class=\"evidence-list\">" + present.map((file) => "<button class=\"evidence-option\" type=\"button\" data-evidence-path=\"" + escapeHtml(file.path) + "\"><strong>" + escapeHtml(recordArea(file.path)) + "</strong><span>" + escapeHtml(file.path) + "</span></button>").join("") + "</div>" : "<p class=\"muted\">No configured Susumu records are present on this branch yet. The first update can create them.</p>";
    $("#repository-evidence").querySelectorAll("[data-evidence-path]").forEach((button) => button.addEventListener("click", () => selectEvidence(button.dataset.evidencePath)));
    selectEvidence(project.sidecar_paths[0]);
    renderAuthoringPanel($(".authoring-tab.active")?.dataset.authoringKind ?? "expectation");
    renderNextWorkRecommendations();
  } catch (error) {
    const scanMessage = error.message.includes("GitHub repository scan failed") ? "GitHub could not read this repository right now. Check the GitHub App installation and repository connection, then try again." : error.message;
    $("#repository-evidence").innerHTML = "<p class=\"form-error\" role=\"alert\">We could not scan the repository records: " + escapeHtml(scanMessage) + "</p>";
  }
}

function renderNextWorkRecommendations() {
  const target = $(".next-work-content");
  if (!target) return;
  const records = state.repositoryFiles.flatMap(parseRecords);
  const expectations = records.filter((record) => record.kind === "expectation");
  const verifications = records.filter((record) => record.kind === "verification");
  const works = records.filter((record) => record.kind === "work");
  const reviews = records.filter((record) => record.kind === "review");
  const recommendations = [];
  expectations.forEach((expectation) => {
    const checks = verifications.filter((record) => record.expectationId === expectation.id);
    const linkedWork = works.some((record) => record.expectationId === expectation.id);
    if (!checks.length) recommendations.push({ label: "Verify", title: expectation.title, detail: "No verification is linked yet." });
    else if (checks.some((record) => ["failed", "inconclusive"].includes(record.status))) recommendations.push({ label: "Review", title: expectation.title, detail: "A linked verification needs attention." });
    else if (!linkedWork) recommendations.push({ label: "Connect work", title: expectation.title, detail: "No implementation work is linked yet." });
  });
  reviews.filter((record) => record.status === "open").forEach((record) => recommendations.push({ label: "Resolve", title: record.title, detail: "An open review conversation needs a next action." }));
  const visible = recommendations.slice(0, 4);
  target.innerHTML = `<p class="eyebrow">Next work</p><h3>What still needs attention?</h3>${visible.length ? `<div class="recommendation-list">${visible.map((item) => `<div class="recommendation"><span class="pill pending">${escapeHtml(item.label)}</span><div><strong>${escapeHtml(item.title)}</strong><p>${escapeHtml(item.detail)}</p></div></div>`).join("")}</div>` : "<p class=\"muted\">No immediate gaps were found in the scanned records.</p>"}`;
}

function selectEvidence(path) {
  const file = state.repositoryFiles.find((item) => item.path === path);
  const evidence = $("#repository-evidence");
  if (!file || !evidence) return;
  evidence.querySelectorAll("[data-evidence-path]").forEach((button) => button.classList.toggle("active", button.dataset.evidencePath === path));
  renderEvidenceRecords(file);
}

function parseRecordLine(line, path, lineNumber) {
  const prefix = line.match(/^(expectation|verification|work|decision|review)\s+(\S+)/);
  const status = line.match(/\bstatus=(\S+)/);
  const title = line.match(/\btitle="([^"]*)"/);
  const detail = line.match(/\bdetail="([^"]*)"/);
  if (!prefix || !status || !detail) return null;
  const source = line.match(/\bfile="([^"]+)"(?:\s+line=(\d+))?/);
  const expectation = line.match(/\bexpectation=(\S+)/);
  const target = line.match(/\btarget=(\S+)/)?.[1] ?? null;
  const subject = line.match(/\bsubject=(\S+)/)?.[1] ?? null;
  const anchor = line.match(/\banchor=(\S+)/)?.[1] ?? null;
  const parent = line.match(/\bparent=(\S+)/)?.[1] ?? null;
  const commentType = line.match(/\bkind=(\S+)/)?.[1] ?? "comment";
  const owner = line.match(/\bowner="([^"]*)"/)?.[1] ?? null;
  return { kind: prefix[1], id: prefix[2], status: status[1], title: title?.[1] || `${prefix[1]} record`, detail: detail[1], expectationId: expectation?.[1] ?? null, target, subject: subject === "-" ? null : subject, anchor: anchor === "-" ? null : anchor, parent: parent === "-" ? null : parent, commentType, owner, path, lineNumber, sourcePath: source?.[1] ?? null, sourceLine: source?.[2] ?? null };
}

function parseRecords(file) {
  return (typeof file.content === "string" ? file.content : "").split(/\r?\n/).map((line, index) => parseRecordLine(line, file.path, index + 1)).filter(Boolean);
}

function fuzzyMatch(record, query) {
  const needle = query.trim().toLowerCase();
  if (!needle) return true;
  const haystack = `${record.kind} ${record.id} ${record.title} ${record.detail}`.toLowerCase();
  let cursor = 0;
  for (const character of needle) { cursor = haystack.indexOf(character, cursor); if (cursor < 0) return false; cursor += 1; }
  return true;
}

function renderRecordList(container, records, query = "") {
  const visible = records.filter((record) => fuzzyMatch(record, query));
  container.querySelector(".record-list").innerHTML = visible.length ? visible.map((record) => `<button class="record-card" type="button" data-record-index="${records.indexOf(record)}"><div class="record-card-heading"><span class="eyebrow">${escapeHtml(record.kind)}</span><span class="pill">${escapeHtml(record.status)}</span></div><h4>${escapeHtml(record.title)}</h4><p>${escapeHtml(record.detail)}</p><small>${escapeHtml(record.id)}</small></button>`).join("") : "<p class=\"muted\">No records match that search.</p>";
  const resultStatus = container.querySelector(".record-search-status");
  if (resultStatus) resultStatus.textContent = `${visible.length} result${visible.length === 1 ? "" : "s"}`;
  container.querySelectorAll("[data-record-index]").forEach((button) => button.addEventListener("click", () => openRecordDetail(records[Number(button.dataset.recordIndex)])));
}

let searchTimer;
let searchSequence = 0;

async function searchRecordsFromApi(container, file, query) {
  const sequence = ++searchSequence;
  const params = new URLSearchParams({ q: query, base_branch: state.selected.allowed_base_branches[0], path: file.path, limit: "50" });
  try {
    const result = await api(`/api/projects/${encodeURIComponent(state.selected.project_key)}/search?${params}`);
    if (sequence !== searchSequence) return;
    renderRecordList(container, result.results ?? []);
    const status = container.querySelector(".record-search-status");
    if (status) status.textContent = `${result.total} result${result.total === 1 ? "" : "s"} · API search`;
  } catch {
    if (sequence !== searchSequence) return;
    const localRecords = parseRecords(file);
    renderRecordList(container, localRecords, query);
    const status = container.querySelector(".record-search-status");
    if (status) status.textContent += " · local fallback";
  }
}

function renderEvidenceRecords(file) {
  const records = parseRecords(file);
  const existing = $("#repository-records");
  if (existing) existing.remove();
  const details = document.createElement("div");
  details.id = "repository-records";
  details.innerHTML = records.length ? `<label class="record-search">Search<input type="search" inputmode="search" placeholder="Type to filter records" /><small class="record-search-status" aria-live="polite"></small></label><div class="record-list"></div>` : "<p class=\"muted\">This evidence area is present but has no readable authored records yet.</p>";
  $("#repository-evidence").append(details);
  if (records.length) {
    renderRecordList(details, records);
    const input = details.querySelector("input");
    input.addEventListener("input", (event) => {
      clearTimeout(searchTimer);
      const query = event.target.value;
      renderRecordList(details, records, query);
      searchTimer = setTimeout(() => searchRecordsFromApi(details, file, query), 240);
    });
    searchRecordsFromApi(details, file, "");
  }
}

function reviewThreadsFor(record) {
  const reviews = state.repositoryFiles.flatMap(parseRecords).filter((item) => item.kind === "review");
  const anchor = `${record.kind}:${record.id}`;
  return reviews.filter((item) => item.anchor === anchor || (!item.anchor && record.kind === "project" && item.target === "project" && !item.subject));
}

function threadChildren(threads, parent) {
  return threads.filter((thread) => (thread.parent ?? null) === parent);
}

function renderThreadNode(thread, threads, depth = 0) {
  const replies = threadChildren(threads, thread.id);
  return `<article class="thread-item" style="--thread-depth:${Math.min(depth, 3)}"><div class="thread-heading"><span class="pill ${thread.status === "open" ? "pending" : "idle"}">${escapeHtml(thread.status)}</span><strong>${escapeHtml(thread.title)}</strong></div><p>${escapeHtml(thread.detail)}</p><small>${escapeHtml(thread.owner || "Unassigned")} · ${escapeHtml(thread.id)}</small><div class="thread-actions"><button class="button button-quiet thread-reply" type="button" data-thread-id="${escapeHtml(thread.id)}">Reply</button></div>${replies.map((reply) => renderThreadNode(reply, threads, depth + 1)).join("")}</article>`;
}

function renderRecordThreads(record) {
  const threads = reviewThreadsFor(record);
  const roots = threads.filter((thread) => !thread.parent || !threads.some((candidate) => candidate.id === thread.parent));
  return `<section class="detail-panel thread-panel"><div class="thread-panel-heading"><div><p class="eyebrow">Review threads</p><h3>${threads.length ? `${threads.length} discussion${threads.length === 1 ? "" : "s"}` : "Start the discussion"}</h3></div><button id="new-record-thread" class="button button-quiet" type="button">New thread</button></div>${threads.length ? `<div class="thread-list">${roots.map((thread) => renderThreadNode(thread, threads)).join("")}</div>` : "<p class=\"muted\">No discussion is anchored to this record yet.</p>"}</section>`;
}

function openRecordDetail(record) {
  const project = state.selected;
  const source = record.sourcePath ? `<div class="source-location"><strong>${escapeHtml(record.sourcePath)}</strong>${record.sourceLine ? `<span>line ${escapeHtml(record.sourceLine)}</span>` : ""}</div><p class="muted">Read-only source preview will appear here when this record is anchored to repository code.</p>` : "<p class=\"muted\">This record has no source location attached. Its portable record and provenance remain the source of truth.</p>";
  const next = record.kind === "expectation" ? "Add a verification that checks this expectation." : record.kind === "verification" ? "Review the result and add work or a review comment if attention is needed." : record.kind === "review" ? "Resolve the conversation or connect it to actionable work." : "Review the linked verification and current synchronization state.";
  $("#project-detail").innerHTML = `<button id="back-to-memory" class="button button-quiet back-button" type="button">← Back to project memory</button><section class="card record-detail-card"><div class="record-detail-heading"><div><p class="eyebrow">${escapeHtml(record.kind)} · ${escapeHtml(record.id)}</p><h2>${escapeHtml(record.title)}</h2></div><span class="pill">${escapeHtml(record.status)}</span></div><p class="record-detail-copy">${escapeHtml(record.detail)}</p><div class="detail-grid"><section class="detail-panel"><p class="eyebrow">Timeline</p><ol class="timeline"><li><span class="timeline-dot"></span><div><strong>Record authored</strong><p>Provenance: human-authored through the Susumu portal.</p><small>Timestamp not recorded in this portable record</small></div></li><li><span class="timeline-dot"></span><div><strong>Repository evidence</strong><p>${escapeHtml(record.path)} on ${escapeHtml(project.allowed_base_branches[0])}</p><small>Current branch inspection</small></div></li></ol></section><section class="detail-panel"><p class="eyebrow">Source context</p>${source}</section></div></section><section class="card next-work-card"><div><p class="eyebrow">Next work</p><h3>${escapeHtml(next)}</h3><p class="muted">Continue from the structured authoring forms without editing raw sidecar syntax.</p></div><button id="detail-next-work" class="button button-primary" type="button">Open Work entry</button></section>`;
  $("#project-detail").querySelector(".record-detail-card").insertAdjacentHTML("beforeend", renderRecordThreads(record));
  $("#back-to-memory").addEventListener("click", renderDetail);
  $("#detail-next-work").addEventListener("click", () => { renderDetail(); setTimeout(() => { renderAuthoringPanel("work"); $("#authoring-panel").scrollIntoView({ behavior: "smooth", block: "start" }); }, 0); });
  $("#new-record-thread").addEventListener("click", () => { renderDetail(); setTimeout(() => { renderAuthoringPanel("review", { anchorKind: record.kind, anchorId: record.id }); $("#authoring-panel").scrollIntoView({ behavior: "smooth", block: "start" }); }, 0); });
  $("#project-detail").querySelectorAll(".thread-reply").forEach((button) => button.addEventListener("click", () => { renderDetail(); setTimeout(() => { renderAuthoringPanel("review", { anchorKind: record.kind, anchorId: record.id, parent: button.dataset.threadId }); $("#authoring-panel").scrollIntoView({ behavior: "smooth", block: "start" }); }, 0); }));
  $("#project-detail").querySelectorAll(".thread-reply").forEach((replyButton) => {
    const thread = reviewThreadsFor(record).find((candidate) => candidate.id === replyButton.dataset.threadId);
    if (!thread) return;
    const action = document.createElement("button");
    action.className = "button button-quiet thread-action";
    action.type = "button";
    action.textContent = thread.status === "open" ? "Resolve" : "Reopen";
    action.addEventListener("click", async () => {
      action.disabled = true;
      try {
        const result = await api(`/api/projects/${encodeURIComponent(project.project_key)}/threads/${encodeURIComponent(thread.id)}/actions`, { method: "POST", body: JSON.stringify({ base_branch: project.allowed_base_branches[0], action: thread.status === "open" ? "resolve" : "reopen" }) });
        showNotice(result.status === "pending" ? "The thread action is in the active pull request and ready for human review." : `Synchronization is ${result.status}.`);
        await loadRepositoryEvidence(project);
        openRecordDetail(record);
      } catch (actionError) {
        showNotice(actionError.message);
        action.disabled = false;
      }
    });
    replyButton.parentElement.append(action);
  });
}

async function loadProjects() {
  state.projects = await api("/api/projects");
  if (state.selected) state.selected = state.projects.find((project) => project.project_key === state.selected.project_key) ?? null;
  state.repositoryFiles = [];
  searchSequence += 1;
  renderProjects();
  renderDetail();
}

function escapeHtml(value) {
  return String(value).replace(/[&<>'"]/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character]);
}

$("#login-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = new FormData(event.currentTarget);
  const error = $("#login-error");
  error.hidden = true;
  try { setSignedIn(await api("/api/auth/login", { method: "POST", body: JSON.stringify({ email: form.get("email"), password: form.get("password") }) })); await loadProjects(); }
  catch (loginError) { error.textContent = loginError.message === "authentication required" ? "The email or password was not recognized." : loginError.message; error.hidden = false; }
});

$("#logout").addEventListener("click", async () => { await api("/api/auth/logout", { method: "POST" }).catch(() => {}); setSignedOut(); });
function applyRepositorySelection(repository) {
  const form = $("#project-form");
  form.elements.github_connection_id.value = repository.github_connection_id;
  form.elements.repository_owner.value = repository.repository_owner;
  form.elements.repository_name.value = repository.repository_name;
  form.elements.installation_id.value = repository.installation_id;
  form.elements.project_key.value = repository.full_name.replace(/[^A-Za-z0-9._-]+/g, "-").toLowerCase();
  form.elements.display_name.value = repository.full_name;
  const branches = form.elements.allowed_base_branches;
  branches.disabled = true;
  branches.innerHTML = '<option value="">Finding branches...</option>';
  const query = new URLSearchParams({ connection_id: repository.github_connection_id, owner: repository.repository_owner, repository: repository.repository_name });
  api("/api/github/branches?" + query.toString()).then((names) => {
    branches.innerHTML = names.map((name) => "<option value=\"" + escapeHtml(name) + "\">" + escapeHtml(name) + "</option>").join("");
    branches.value = repository.default_branch || names[0] || "";
    branches.disabled = !names.length;
  }).catch((error) => {
    branches.innerHTML = '<option value="">Could not load branches</option>';
    $("#project-error").textContent = error.message;
    $("#project-error").hidden = false;
  });
}

async function openProjectDialog() {
  $("#project-error").hidden = true;
  const form = $("#project-form");
  const selector = form.elements.repository_selector;
  selector.innerHTML = '<option value="">Finding repositories...</option>';
  $("#project-dialog").showModal();
  try {
    const repositories = await api("/api/github/repositories");
    const existing = new Set(state.projects.map((project) => (project.repository_owner + "/" + project.repository_name).toLowerCase()));
    const available = repositories.filter((repository) => !existing.has(repository.full_name.toLowerCase()));
    selector.innerHTML = available.length ? '<option value="">Choose a repository</option>' + available.map((repository, index) => "<option value=\"" + index + "\">" + escapeHtml(repository.full_name) + (repository.private ? " · Private" : "") + "</option>").join("") : '<option value="">All available repositories are connected</option>';
    selector.disabled = !available.length;
    selector.onchange = () => { const repository = available[Number(selector.value)]; if (repository) applyRepositorySelection(repository); };
    if (available.length === 1) { selector.value = "0"; applyRepositorySelection(available[0]); }
  } catch (error) {
    selector.innerHTML = '<option value="">Could not load repositories</option>';
    selector.disabled = true;
    $("#project-error").textContent = error.message;
    $("#project-error").hidden = false;
  }
}

async function loadProjectRepositories(connectionId) {
  const path = connectionId ? "/api/github/repositories?connection_id=" + encodeURIComponent(connectionId) : "/api/github/repositories";
  const repositories = await api(path);
  const existing = new Set(state.projects.map((project) => (project.repository_owner + "/" + project.repository_name).toLowerCase()));
  return repositories.filter((repository) => !existing.has(repository.full_name.toLowerCase()));
}

openProjectDialog = async function () {
  $("#project-error").hidden = true;
  const form = $("#project-form");
  const connectionSelector = form.elements.github_connection_selector;
  const selector = form.elements.repository_selector;
  connectionSelector.innerHTML = '<option value="">Finding GitHub connections...</option>';
  selector.innerHTML = '<option value="">Choose a connection first</option>';
  $("#project-dialog").showModal();
  try {
    state.githubConnections = await api("/api/github/connections");
    connectionSelector.innerHTML = state.githubConnections.length ? '<option value="">Choose a GitHub connection</option>' + state.githubConnections.map((connection) => "<option value=\"" + escapeHtml(connection.id) + "\">" + escapeHtml(connection.name) + "</option>").join("") : '<option value="">No GitHub connections configured</option>';
    connectionSelector.disabled = !state.githubConnections.length;
    connectionSelector.onchange = async () => {
      selector.innerHTML = '<option value="">Finding repositories...</option>';
      try {
        const available = await loadProjectRepositories(connectionSelector.value);
        selector.innerHTML = available.length ? '<option value="">Choose a repository</option>' + available.map((repository, index) => "<option value=\"" + index + "\">" + escapeHtml(repository.full_name) + "</option>").join("") : '<option value="">All available repositories are connected</option>';
        selector.disabled = !available.length;
        selector.onchange = () => { const repository = available[Number(selector.value)]; if (repository) applyRepositorySelection(repository); };
        if (available.length === 1) { selector.value = "0"; applyRepositorySelection(available[0]); }
      } catch (error) { $("#project-error").textContent = error.message; $("#project-error").hidden = false; }
    };
  } catch (error) {
    connectionSelector.innerHTML = '<option value="">Could not load connections</option>';
    selector.disabled = true;
    $("#project-error").textContent = error.message;
    $("#project-error").hidden = false;
  }
};

$("#new-project").addEventListener("click", openProjectDialog);
$("#close-project-dialog").addEventListener("click", () => $("#project-dialog").close());
$("#cancel-project-dialog").addEventListener("click", () => $("#project-dialog").close());
$("#add-github-connection").addEventListener("click", () => { $("#project-dialog").close(); $("#connection-form").reset(); $("#connection-error").hidden = true; $("#connection-dialog").showModal(); });
$("#close-connection-dialog").addEventListener("click", () => $("#connection-dialog").close());
$("#cancel-connection-dialog").addEventListener("click", () => $("#connection-dialog").close());
$("#connection-form").addEventListener("submit", async (event) => {
  if (event.submitter?.value === "cancel") return;
  event.preventDefault();
  const form = new FormData(event.currentTarget);
  const error = $("#connection-error");
  error.hidden = true;
  try {
    await api("/api/github/setup", { method: "POST", body: JSON.stringify({ name: form.get("name"), app_id: Number(form.get("app_id")), private_key_pem: form.get("private_key_pem") }) });
    $("#connection-dialog").close();
    await openProjectDialog();
  } catch (connectionError) { error.textContent = connectionError.message; error.hidden = false; }
});
$("#project-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const projectForm = event.currentTarget;
  const form = new FormData(projectForm);
  const error = $("#project-error");
  error.hidden = true;
  try {
    await api("/api/projects", { method: "POST", body: JSON.stringify({ project_key: form.get("project_key"), display_name: form.get("display_name"), provider: "github", repository_owner: form.get("repository_owner"), repository_name: form.get("repository_name"), installation_id: Number(form.get("installation_id")), github_connection_id: form.get("github_connection_id") || undefined, allowed_base_branches: [form.get("allowed_base_branches")] }) });
    $("#project-dialog").close(); projectForm.reset(); await loadProjects(); showNotice("Repository connected. Its synchronization states are ready.");
  } catch (projectError) { error.textContent = projectError.message; error.hidden = false; }
});

async function restoreWorkspace() {
  try {
    const user = await api("/api/me");
    setSignedIn(user);
  } catch {
    setSignedOut();
    return;
  }
  try {
    await loadProjects();
  } catch (error) {
    showNotice(`Your session is active, but the workspace could not load: ${error.message}`, true);
    $("#project-list").innerHTML = '<p class="form-error" role="alert">The workspace could not load its repository list. Refresh to try again.</p>';
  }
}

restoreWorkspace();
