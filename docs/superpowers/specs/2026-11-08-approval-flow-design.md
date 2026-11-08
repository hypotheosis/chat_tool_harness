# Tool approval flow: one decision per turn, trusted servers

Status: approved for implementation on branch `approval-flow` (2026-11-08).
Amends: `2026-11-07-android-mcp-agent-design.md` (permissions: "new tools
default to ask"; schema changes revoke Allow) for servers the user marks as
trusted, and `2026-11-08-user-testing-fixes-design.md` §8 (chat screen).

## Background

On-device testing with a real MCP server showed the approval flow is unusable:

- The approval request lives inside a collapsed tool card in the transcript;
  the run row offers only Resume and Stop. Resuming without a decision returns
  straight to "Waiting for approval", so Resume appears to do nothing.
- A model that requests two calls in one turn gets two identical cards, but
  only the first has an approval binding, so the second card has no buttons.
- Approving does not continue the run: the user approves, scrolls to Resume,
  approves the next call, resumes again.
- Approve/Deny stay visible after approval; a second tap fails with "stale
  approval", because closing the approval's MCP session clears observed tool
  definitions.
- Cards show profile, server and attempt UUIDs instead of names.

The user also wants to trust their own MCP server outright, including tools
it adds later.

## 1. Bindings for every pending call in a turn

When the run loop reaches an attempt whose authorization is Ask, it also
prepares approval bindings for every later `Proposed` attempt in the same run
whose policy is Ask and that has no binding yet, before entering
`WaitingApproval`. Preparing a binding stores it only; it never changes an
attempt's state and never dispatches. Attempts whose policy is Allow or Block
are left for the loop as today. So every call that needs a decision can be
decided at once.

## 2. Server trust

- New table `trusted_servers(server_id TEXT PRIMARY KEY REFERENCES servers(id),
  destination_digest TEXT NOT NULL)` (migration v8). Trust is per server,
  app-wide (every profile using the server).
- `SetServerTrust { server_id, trusted }` stores the server's current
  destination digest (URL, HTTP flag and secret-header references, the same
  digest Allow rules bind to) or deletes the row. It needs no network.
- A server is trusted iff its row exists and its digest equals the server's
  current destination digest. Changing the URL, HTTP flag or credential
  references silently ends trust (the row stays but no longer matches; saving
  trust again replaces it).
- Policy order in `authorize`: a Block rule wins; else an Allow rule bound to
  the current definition, or a trusted server, means Allow; else Ask. Trust
  covers new tools and changed schemas: definitions must still come from
  current discovery (observed) and arguments must still validate against the
  schema. Local file tools are never covered by server trust.
- When policy is Allow (by rule or trust), a `Proposed` attempt dispatches even
  if a binding was stored earlier, so trusting a server while calls wait lets
  them run on resume.
- `AgentServerConfig` gains `trusted: bool` (computed as above).

## 3. Deciding several calls at once

- New command `DecideApprovals { run_id, approve: Vec<ApprovalBinding>, deny:
  Vec<AttemptId> }`: approves and denies the given attempts of one run, then
  resumes the run. Approval of MCP calls re-verifies definitions with one
  discovery per server involved (not one per call). Any stale binding fails
  the whole command with Conflict and changes nothing. Resume runs only after
  every decision is stored.
- `Approve`, `Deny` and `ResumeRun` stay for compatibility and tests.
- Repeating a decision for an attempt already in that state is not an error
  (idempotent), so a double tap cannot produce "stale approval".

## 4. Chat screen

- A run waiting for approval shows one approval card above the composer (like
  the disclosure card), listing every pending call: server name, tool name
  and arguments (pretty-printed, collapsible when long). Actions:
  - **Approve all** (primary): `DecideApprovals` approving every pending call.
  - **Deny all**: denying every pending call.
  - **Always allow <server>** when every pending call is from one MCP server:
    a confirmation ("Tools from <server> will run without asking, including
    tools it adds later, until you turn this off in its settings or its URL or
    credentials change."), then `SetServerTrust` and Approve all.
  - Per call, a small Deny, so a duplicate can be dropped while the rest run.
- Tool cards in the transcript show `tool · server name · state` (no UUIDs);
  profile, server and attempt ids, destination and arguments move under the
  card's details. Cards no longer carry Approve/Deny.
- The run row for `WaitingApproval` reads "Waiting for your approval" with Stop
  only. Resume appears there only when no pending call still needs a decision
  (for example the app closed between approving and resuming).
- A call that is queued behind others with no decision needed shows
  "Queued".

## 5. Settings

- The MCP server screen gets a "Trust this server" switch with the same
  explanation as the confirmation, reflecting `trusted`.
- The tool rules screen notes "Trusted server: all tools run without asking"
  for a trusted server; Block rules still apply and remain editable.

## Testing

- Rust: bindings prepared for every pending Ask call and no state change;
  trusted server allows new and schema-changed tools; trust ends when the URL
  or a credential reference changes; Block beats trust; trust never covers
  local files; `DecideApprovals` is all-or-nothing, idempotent for repeats,
  and resumes once; migration v8.
- Widgets (fake backend): approval card lists every pending call with names,
  Approve all / Deny all / Always allow send the right commands, per-call Deny,
  no Resume while a decision is pending, no UUIDs in card titles, server
  trust switch.
- Real bridge on the emulator: a run with two calls is approved with one
  `DecideApprovals` and completes; a trusted server's call runs without a
  prompt.
