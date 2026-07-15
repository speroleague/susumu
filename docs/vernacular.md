# Susumu vernacular

Susumu should make project knowledge easier to trust by saying what kind of statement each record is making. The scanner must not sound like a person giving advice, and optional AI must not sound like observed fact.

## Source categories

| Category | Meaning | Preferred voice |
| --- | --- | --- |
| Scanner-observed | Facts parsed or measured directly from source files. | “Observed route `POST /checkout`.” |
| Susumu-derived | Deterministic analysis calculated from observed records. | “Workflow attention score includes 5 outgoing calls.” |
| Human-authored | Intent, expectations, decisions, review notes, or business language entered by people. | “Checkout must reserve inventory before charging.” |
| Imported | Records copied from external systems such as CI, issue trackers, policy tools, or requirement stores. | “CI run `123456` reported passed.” |
| Verification-reported | A check result linked to an expectation. | “Verification `v_checkout` reported failed.” |
| AI-suggested | Optional generated hypotheses, summaries, clusters, or drafts. | “AI draft suggests a missing expectation.” |

Every record should make provenance inspectable through a `source`, stable target id, or both.

## Scanner voice rules

Scanner-generated text should be factual, bounded, and unemotional.

- Prefer “observed,” “resolved,” “unresolved,” “linked,” “not found,” “matched,” and “reported.”
- Avoid “should,” “must,” “needs,” “bad,” “messy,” “broken,” “risky,” and “important” in scanner-generated findings.
- Do not imply business priority from static evidence. Use “attention score” or “look here first,” not “business priority.”
- Do not imply correctness from static evidence. Use “passed verification linked” rather than “requirement satisfied.”
- Do not imply intent from implementation. Code can show behavior; expectations show intent.
- Preserve uncertainty as a first-class result. If a target is ambiguous or external, say so and keep the gap.

## Allowed voices by record

### `file`, `symbol`, `dependency`, `workflow`, and `flow`

These are scanner-observed records. They should describe what was parsed or resolved.

Good:

```text
HTTP route observed
handler symbol resolved
3 unresolved outgoing call edges
```

Avoid:

```text
Important business workflow
Needs implementation evidence
This handler is too complicated
```

### `attention`

`attention` is Susumu-derived. It is an attention score, not a business priority score.

Good:

```susu
attention workflow=w_checkout source="susumu:derived" score=110 detail="workflow trigger observed; handler symbol resolved; HTTP route observed; accepted expectation linked; failed verification linked";
```

Avoid:

```susu
attention workflow=w_checkout source="susumu:derived" score=110 detail="critical business workflow; needs fixing";
```

### `expectation`

Expectations are authored or imported intent. Human and policy language may use “must,” “should,” and other normative words because the source is making a requirement claim, not the scanner.

Good:

```susu
expectation e_checkout target=workflow subject=w_checkout status=accepted source="human:product" title="Checkout reserves inventory before charging" detail="The checkout workflow must reserve inventory before payment capture.";
```

### `verification`

Verifications report checks. They should say who or what performed the check, the method, the reported status, and the evidence reference.

Good:

```susu
verification v_checkout expectation=e_checkout status=failed method="manual workflow review" source="human:engineer" evidence="review:42" basis=abc123 detail="Review reported unresolved inventory and payment edges.";
```

When a verification includes a `basis` fingerprint, Susumu may later emit a scanner-derived stale-review finding if the expectation target evidence changes. That finding means "rerun or review this check against new evidence," not "the verification was false."

### `decision`

Decisions are authored judgment. They can approve, reject, supersede, or propose a choice about a workflow, expectation, file, symbol, or whole project. They may use business language because the source is explicitly making a judgment claim.

Good:

```susu
decision d_checkout_exception target=workflow subject=w_checkout status=accepted source="human:director" title="Accept checkout exception" detail="The team accepts this exception for the current release with follow-up verification required.";
```

Avoid treating decisions as scanner evidence:

```text
Decision accepted, therefore implementation is correct.
```

When a decision includes a `basis` fingerprint, Susumu may later emit a scanner-derived stale-review finding if the targeted evidence changes. That finding means “review the decision against new evidence,” not “the decision is wrong.”

### `work`

Work records are activity history. They say what a human, AI agent, import, or automation claims was done, which target it touched, and which expectation it may address. They do not prove the expectation passed.

Good:

```susu
work wk_checkout_agent target=workflow subject=w_checkout expectation=e_checkout kind=implementation status=completed source="agent:codex" evidence="commit:abc123" title="Update checkout reservation" detail="Updated checkout so inventory reservation happens before payment capture.";
```

Avoid treating work as verification:

```text
Work completed, therefore requirement satisfied.
```

If work is AI-authored, use provenance such as `source="agent:codex"` or `source="ai:draft"`. The work record can be useful without being authoritative.

### `finding`

Findings are deterministic rule results. They may mark an attention point, gap, or relationship problem, but they should not prescribe action.

Good:

```text
Observed checkout coordinating 9 internal units. High fan-out marks a code-change attention point.
```

Avoid:

```text
Checkout is messy and should be refactored.
```

## Optional AI vocabulary

AI output is allowed only as labeled assistance. It can summarize, cluster, draft expectations, suggest names, or propose missing review questions. It cannot silently promote a hypothesis into scanner-observed evidence.

Preferred labels:

- `source="ai:draft"` for generated expectation drafts.
- `source="ai:summary"` for generated summaries.
- `status=proposed` until a human, policy import, deterministic scanner, or verification process accepts it.

If the AI cites evidence, the cited evidence remains the source of the fact. The AI record is the source of the wording or hypothesis.
