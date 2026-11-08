# User-testing fixes: diagnosability, credentials, live profile, chat cleanup

Status: implemented on branch `user-testing-fixes` (2026-11-08); amendments from implementation review are marked "Amended".
Amends: `2026-11-07-android-mcp-agent-design.md` (profile snapshots, disclosure).

## Background

First on-device user testing of the core debug build failed repeatedly on
configuration mistakes the app could not explain:

- Endpoint and MCP URLs entered without their API path produced 404s reported
  only as "core transport unavailable".
- An MCP `Authorization` secret stored without `Bearer ` produced 401s; the
  connection test showed nothing because settings screens never render errors.
- Fixing settings did not affect an existing conversation, which kept an
  immutable profile snapshot pointing at a deleted credential, failing locally
  with "core authentication failure".
- The chat screen was dominated by disclosure, revision and run-id text, with
  no clear next action after a failure.

This change makes every failure say what failed and what to do, makes
credentials predictable, makes conversations follow current settings, and
declutters the chat and settings screens.

## 1. Live profile (approach A: automatic snapshot refresh)

Conversation profile snapshots stay as the record of what each run used, but
they stop being user-managed.

- `refresh_profile(conversation)` resolves the conversation's profile against
  current profile, endpoint and server rows. If the result differs from the
  conversation's current snapshot payload (ignoring `revision`), it inserts a
  new revision exactly as `apply_profile` does today; otherwise it does nothing.
- It runs at the start of a new turn, a summary request, and a resume.
- Resume re-binds the run to the newest revision (`runs.snapshot_revision` is
  updated in the same transaction) so a fixed URL or credential applies to a
  paused run. Unknown-outcome tool attempts are still never replayed.
  Amended: the refresh commits in its own transaction first; the rebind and the
  state change to Running are one transaction guarded by the run's state, so a
  concurrent cancel leaves the cancelled run's revision untouched.
- The UI "Apply current profile" action and its explanatory text are removed.
  `AppCommand::ApplyProfile` remains for switching a conversation to a
  different profile id.
- If the profile was deleted, refresh fails with `Configuration` and the
  failure detail names the profile.

The original design line "Starting a conversation copies a profile snapshot.
Editing a profile does not silently change old conversation ..." is superseded:
editing settings changes future turns and resumes of every conversation using
that profile; run history keeps the revision each run actually used.

## 2. Disclosure: once per origin, app-wide

- New table `acknowledged_origins(origin TEXT PRIMARY KEY)`, where origin is
  the normalized `scheme://host[:port]` of the inference endpoint URL.
- A run or summary needs disclosure iff its endpoint's origin is not in the
  table. Path, model, credential and profile changes never re-prompt; a new
  host, scheme or port does.
- Acknowledging stores the origin. Amended: any origin the conversation has
  used (in any snapshot revision, newest first, skipping unreadable ones) may
  be acknowledged, so a paused run on an older revision can be allowed.
- Migration copies existing per-conversation acknowledgments into the table by
  reducing their destination to its origin; the old table is left in place but
  no longer read.
- `DisclosureAcknowledgment` becomes `{ origin }`; `ContextChoiceReason::
  DisclosureRequired` is kept.

## 3. Failure detail

New serializable type, attached where a failure is observed:

```rust
pub struct FailureDetail {
    pub target: FailureTarget,      // Inference { name } | McpServer { name } | Profile { name } | Credential
    pub step: FailureStep,          // DiscoverModels | ChatRequest | Initialize | ListTools | CallTool | ResolveCredential | RefreshProfile
    pub url: Option<String>,        // request URL without query, userinfo or fragment
    pub status: Option<u16>,        // HTTP status if a response arrived
    pub reason: Option<String>,     // <= 160 chars, sanitized
}
```

The detail URL keeps its path: it is the URL the user configured (plus the
request suffix), so the user can compare it with their settings.

- `reason` comes from the response body's JSON `error.message`, `error`
  (string) or `message`, read within the existing response limits; otherwise
  from the transport error class: `dns`, `connect`, `tls`, `timeout`, `body`.
  Control characters are stripped, length is capped at 160 characters on a
  char boundary, with the request's resolved secrets (inference credential and
  MCP header values) and any 32+ character token-like word replaced by •••.
- `CoreError` keeps its kinds and `CoreResult` is unchanged. Lower layers
  record the detail with a task-local note (`rust/src/failure.rs`); run
  execution, diagnostics and pre-launch preparation run inside a failure scope
  (`scope`, or `sync_scope` on blocking threads) and read the last note at
  their boundary. The bridge's `agent_command` returns
  `AgentFailure { code, detail }`.
- A missing credential reference gets its own kind, `CredentialMissing`,
  instead of `Authentication`. Amended: on Android a reference whose file is
  absent resolves to "missing"; decrypt, keystore and unreadable-file errors
  stay authentication failures. `AgentRun.failure` holds the sanitized
  reason, or the code's text when there is none; `failure_code` and
  `failure_detail` carry the rest. Amended: a credential-lookup note names the
  endpoint or MCP server that holds the credential (network code re-targets
  the `Credential` note at the holder), so the action opens that settings
  screen.
- `runs.failure` stores the code and, only when something noted the failure,
  the detail as JSON. Amended: a failure nothing noted (for example a JSON-RPC
  error, a validation conflict or a local limit) stores no detail rather than
  a guessed target; the card shows the plain per-code text and opens the
  profile. Old string values still read as that code without detail. The bridge error carries `{ code, detail }`; Dart's
  `BackendFailure` gains `detail`.
- Connection tests (model discovery, MCP discovery) return the same detail.

## 4. Credentials

- One rule for both the inference credential and an MCP header named
  `Authorization` (case-insensitive): if the stored value begins with an auth
  scheme token followed by a space and more text
  (`^[A-Za-z][A-Za-z0-9!#$%&'*+.^_`|~-]* .`), send it verbatim; otherwise send `Bearer <value>`. Other header names are
  always verbatim.
- New platform call `describeCredential(reference)` returns
  `{ scheme: Option<String>, length, sendable }` computed on-device after
  decryption; the secret never crosses into Dart. Amended: `sendable` is false
  unless every byte is visible ASCII or a single space between visible bytes,
  which flags pasted newlines, tabs and stray spaces. Settings show the
  effective header with the secret's length, e.g.
  `Authorization: Bearer ••• (40 chars)`, `(47 chars; Bearer added)`, a warning
  when not sendable, "No credential stored", or "Stored credential cannot be
  read; re-enter it." Kotlin and Rust share one table of test cases.

## 5. Endpoint defaults

New endpoints default to 32768 context tokens and 2048 max output tokens
(previously 4096 / 512). Existing endpoints are unchanged.

## 6. Settings screens show results

- A shared `ResultBanner` widget renders the controller's last error or
  success under the action buttons on the endpoint, server, profile and tool
  rules screens, replacing ad-hoc `result` text.
- Connection tests end in exactly one visible state: success with a count
  ("Connected to <name>: N tools available" / "N models available") or the
  failure sentence with a Details toggle (target, step, URL, status, reason).
- The stale "secure token entry arrives in Task 12" message is removed.
- Amended: a new result scrolls itself into view (only when it changes, not
  when a lazy list rebuilds the banner); "Reload" appears only when the
  screen's own load failed; progress shows in the app bar, not under the
  navigation bar.

## 7. Messages and actions

`failureMessage(failure)` builds one sentence from the detail when present
(e.g. `MCP server "<name>": HTTP 401 during initialize: unknown API key.`) and
otherwise falls back to plain per-code text. Each code maps to one action:

| Code | Action |
|---|---|
| authentication | Open the target's settings |
| credentialMissing | Re-enter credential (target settings) |
| unavailableTransport, protocol, configuration | Open the target's settings |
| contextLength | Choose context |
| rateLimited | Retry |
| other | Retry |

## 8. Chat screen

- Header row: profile name and model, with a settings button opening the
  profile. Snapshot explanation, revision text, "Apply current profile" and
  "Repair configuration" are removed.
- Messages and tool-call cards stay.
- Runs: a running run shows a compact "Working…" row with Stop (the
  "stop is local / best effort" note moves into Stop's confirmation dialog).
  A failed or paused run shows one inline card under the triggering message:
  title, one-sentence cause, one action button, and "Details" (run id, target,
  step, URL, status, reason). Successful runs show no run row.
- Disclosure appears only when the endpoint origin is unacknowledged: one
  card above the composer, "Messages will be sent to <host>." with Allow.
- Context and summary-context choosers appear only when the backend reports a
  context choice is needed, as that card's action.
- Amended after review:
  - A run with a Dispatching or OutcomeUnknown tool attempt is never offered
    Retry, Resume or Allow: a failed one shows "A tool call's outcome is
    unknown; check the remote system before continuing." with no action; an
    unfinished one offers Stop only.
  - A paused run whose action opens settings or a context chooser switches to
    Resume once the user returns from that screen or applies a selection.
  - Every paused or interrupted run keeps its card and action; older failed
    runs collapse to one muted line with Details.
  - A disclosure pause offers Allow for the run's own origin, then resumes.
  - Runs carry `input_message_id` so cards sit under their turn; Retry
    re-sends that message as a new turn (it appears twice in the transcript;
    a backend re-run command would be cleaner).

## 9. Safe area on settings screens

Settings screens (endpoint, server, profile, tool rules) and the chat composer
respect system insets: bodies are wrapped in `SafeArea` and scrollable lists
get bottom padding of `MediaQuery.viewPaddingOf(context).bottom` plus the
normal margin, so the last widgets are never hidden behind the Android
navigation bar in edge-to-edge mode.

## 10. Testing

- Rust unit tests: refresh creates a revision only on change; resume re-binds;
  origin acknowledgment ignores path changes and re-prompts on host/port/scheme
  changes; migration preserves acknowledgments; failure detail extraction from
  JSON bodies, truncation, control-char stripping and secret rejection; a
  regression test that a 401 reason survives to the run failure; credential
  scheme rule cases (bare, `Bearer x`, `Basic x`, other header).
- Widget tests with the fake backend: new endpoint defaults; each settings
  screen shows an error after
  a failed test (regression for silent failures); chat failed-run card shows the
  cause and correct action; disclosure card only for an unacknowledged origin;
  no revision text; last settings widget is above a simulated bottom inset.
- Real-bridge integration: existing suite on a device plus one case asserting a
  failure detail survives the bridge.
- Manual device check: reproduce a wrong MCP credential and confirm the screen
  names the server, status and reason.

## Out of scope

Removing snapshots entirely (approach B), OAuth, PDF work, and changes to
permission semantics.
