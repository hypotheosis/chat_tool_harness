# Android MCP chat agent — v1 design

Status: accepted as the implementation baseline; the user subsequently authorized subagent-driven execution. Task 1's emulator feasibility gate passed, with physical/OS-policy checks still open. The user approved revising PDF support around a separate Android process; the detailed [PDF worker amendment](2026-11-07-pdf-worker-design.md) was accepted when the user asked to continue and is the current Task 2 contract. All other unimplemented capabilities below remain requirements, not claims of completed functionality.

## Delivery priority

The user explicitly made PDF the lowest priority on 2026-11-08. Core chat, inference, MCP, profiles, SQLite history, generated Markdown/text files and a usable Android build come first. Existing PDF work is preserved but unaccepted; remaining PDF implementation/verification/UI is last and no longer gates the core application. This changes delivery order, not PDF acceptance standards or authorization to ship an unverified worker.

## Intent

Provide a personal Android chat interface for asking questions about, and making changes through, remote MCP servers. Typical workflows retrieve stored knowledge, investigate application behavior, combine findings across servers, and save a Markdown summary of what a conversation established. This is not initially a coding agent.

The agent runtime and conversation storage live on the phone. Inference happens at user-configured remote endpoints. MCP tools run on their configured servers. “Local runtime” does not mean offline inference or that conversation content never leaves the device.

## Confirmed scope

- Android only initially; continue active work while switching to other apps, within Android execution limits.
- Configurable remote llama.cpp, Ollama, and OpenAI-compatible inference endpoints.
- Configurable HTTP MCP servers with API-key/token authentication; OAuth deferred.
- Reusable profiles containing endpoint/model, system instructions, available MCP servers, and per-tool ask / allow automatically / block rules.
- Separate conversations persisted in device SQLite.
- MCP reads and writes, streaming replies, stop controls, inspectable tool activity, connection diagnostics, bounded runs, and interruption recovery without blindly replaying uncertain writes.
- Visible context limits and explicit summarization; rename, search, delete, and export conversations.
- Incoming PDFs with selectable text; OCR explicitly excluded.
- Generated files, especially Markdown summaries, available to save and share.

## Non-goals and anti-goals

No remote agent hosting, local model inference, shell/code execution, unrestricted filesystem access, scheduled agents, sync, voice, OAuth, OCR, or visual interpretation of PDF diagrams in v1. No promise of perfect PDF layout/table extraction, unlimited document size, exact token counting on every endpoint, or uninterrupted execution after Android stops the process.

Failure modes to prevent: tool calls bypassing permission rules; silently losing conversation/tool state; duplicating a write after a timeout; claiming unread document content was examined; silently discarding context; sending configured credentials to the model; interpreting an unavailable tool as a successful operation.

## Approaches

### Selected: existing Flutter/Rust application with Android lifecycle support

Flutter owns presentation and interactions. Rust owns agent decisions, network clients, permissions, and persistence. Android integration supplies foreground-service lifecycle, notifications, file access, and secure credential storage. This matches the accepted architecture and repository boundaries, while allowing core behavior to be tested without a device.

### Not selected: remotely hosted agent with mobile client

Simplifies long-running work across phone suspension, but adds hosting/authentication and remote transcript storage or synchronization. Conflicts with the user's local-runtime requirement.

### Not selected: native Android rewrite

Offers direct platform integration but discards the existing Flutter/Rust scaffold without a demonstrated benefit. Small Android adapters are sufficient unless a platform feasibility test proves otherwise.

## Existing repository and boundaries

The current application is the greeting scaffold in `lib/main.dart` and `rust/src/api/simple.rs`. `rust/Cargo.toml` has only flutter_rust_bridge as a backend dependency and builds `cdylib`/`staticlib`. `flutter_rust_bridge.yaml` exports `crate::api` into `lib/src/rust/`; FRB is pinned to 2.13.0. The Android runner currently has only a `FlutterActivity`, no agent service or release-manifest network permission, and template debug signing for release builds. These are starting conditions, not completed app capabilities.

Keep `rust/src/api/` thin. Add ordinary Rust modules for profiles/storage, inference, MCP, run state, permissions, and attachments as concrete behavior appears. Dart uses an app-owned backend interface so widget tests can use fakes. No business state machine belongs in widgets or Android service callbacks.

Regenerate FRB output after bridge-visible changes; do not hand-edit generated files or casually change vendored build glue. Library selection, exact module names, and schema DDL belong to implementation planning, not this document.

## Profiles, endpoints, and permissions

The following are proposed design defaults rather than decisions separately dictated by the user:

- Endpoint records store name, base URL, model ID, context-window setting, bounded generation settings, and credential references. Allow manual model IDs when discovery is unavailable.
- MCP connection records store name, endpoint URL, nonsecret connection settings, and secure references for bearer/API-key header values. Secret custom headers use the same secure storage path.
- Profiles reference those records and store system instructions and tool rules. Servers/endpoints can be reused without copying credentials into profiles.
- Starting a conversation copies a profile snapshot. Editing a profile does not silently change old conversation instructions/model/server selection. An explicit “apply updated profile” action changes future turns only and is recorded in history. Superseded 2026-11-08 by `2026-11-08-user-testing-fixes-design.md` §1: conversations refresh their snapshot from current settings before each turn, summary and resume.
- Permission rules are scoped to profile identity, configured server identity, and exact tool name. Conversations keep a historical snapshot, but dispatch checks current rules; changing a tool to block revokes pending execution in existing conversations too. New conversations get a new profile identity only when explicitly duplicating a profile.
- New tools default to ask, whether labeled read-only or not. MCP annotations are hints, not authorization. A changed server destination or changed tool schema invalidates prior automatic-allow trust and returns the affected tools to ask.
- Ask authorizes one exact call: server identity/destination, tool, and immutable arguments. The user can deny it or separately change the persistent rule. A changed argument set requires a new approval. A block prevents dispatch and returns a clear denial to the agent. Automatic allow covers reads or writes only when the user deliberately configures it.
- The Rust dispatch boundary enforces policy on every MCP or built-in tool call. Model output, prompts, PDF text, and tool results cannot edit rules or grant authority.

Blocked MCP tools are omitted from the advertised tool list and still rejected at dispatch if requested. Colliding tool names across servers map to distinct model-visible names and preserve their original routing identity.

## Inference and MCP interoperability

Proposed v1 inference transport: OpenAI-compatible Chat Completions, including streaming text and structured function/tool calls, used against all three endpoint families through their compatibility APIs. Native Ollama and provider-specific Responses APIs are not required initially. This is a compatibility choice to review, not a claim that every server/model implements identical behavior.

Connection tests distinguish connectivity/authentication, optional model discovery, and actual model tool-call support. A model without working structured tool calls may be used for chat-only operation with a clear warning; it must not simulate successful MCP actions. Do not parse arbitrary prose as executable tool calls. Show actionable, sanitized errors for malformed schemas, unsupported parameters, invalid streamed calls, and model failures. Never execute incomplete streamed arguments.

Proposed MCP baseline: Streamable HTTP with initialization, protocol-version negotiation, server session handling where required, paginated tool discovery, and JSON/SSE response handling within that transport. Legacy HTTP+SSE and stdio transports are deferred. MCP resources/prompts and server-initiated sampling are not v1 requirements; reject unsupported requests explicitly. Record advertised capabilities accurately and refresh discovery on reconnect or supported list-change notifications.

No silent endpoint failover: it could send private data somewhere the user did not choose. A disconnected server must not prevent use of other available servers, but its tools are marked unavailable rather than returning fabricated empty results.

HTTPS is the default. Permit HTTP only through explicit per-endpoint opt-in with a warning that tokens and payloads travel unencrypted. Retain normal certificate validation; no global “accept all certificates” option. Do not forward credentials on cross-origin redirects or let an MCP response redirect authenticated requests to an unapproved destination.

## Agent execution and persistence

Proposed v1 concurrency: one active run across the application, with many stored conversations. A run waiting for approval does not hold the execution slot; the user can work in another conversation. Resuming a waiting run reacquires the slot. No invisible run queue, parallel agents, or parallel MCP writes in v1.

A user turn proceeds through: persist input and configuration snapshot; assemble bounded context; request inference; accumulate/persist streaming output; validate requested calls; resolve permissions; execute calls sequentially; persist results; request the next model step; persist final answer and release execution resources.

Persist observable states: running, waiting for approval, paused, completed, cancelled, failed, and interrupted. Tool attempts additionally distinguish proposed, denied, approved, dispatching, succeeded, failed, and outcome unknown. Durable IDs associate messages, provider tool-call IDs, run steps, approvals, and results.

Write a dispatching record before sending a tool request, then write the result transactionally before starting the next step. A crash between server execution and local result storage cannot be solved with a local transaction: dispatched attempts lacking a trustworthy result become outcome unknown. This applies to any possibly side-effecting call, including automatically allowed calls. Exactly-once execution across arbitrary MCP servers is not promised.

For an outcome-unknown attempt, show the exact request and the uncertainty, with a way to open relevant remote context when available. Do not provide a generic retry button for that attempt. After checking remote state, the user can issue a new instruction; this creates a new attempt and follows current permissions rather than replaying the old approval. Server-specific reconciliation is optional future work, not a generic v1 integration.

On process restart, restore the transcript and mark unfinished work interrupted; do not automatically resume network work. The user explicitly resumes. Persisted approvals are not replayed automatically. Use provider/server idempotency or status reconciliation only where actually supported; otherwise require the user to check the remote state before considering another write. No generic automatic retry of tool execution requests in v1. Safe discovery/network setup may use bounded retries.

Stop immediately records cancellation intent, prevents further inference/tool dispatch, cancels outstanding local requests where possible, saves partial text, and releases the service after bounded local teardown. The UI need not wait for a remote response. The run can be cancelled while its already-dispatched tool attempt remains outcome unknown; those are separate states. Cancelling an HTTP request does not undo a server-side action; pending outcomes remain explicit. A denied tool result is reported to the model without executing the call. Malformed tool arguments or budget exhaustion terminate the run with a useful explanation rather than entering an unbounded correction loop.

## Android background execution

The foreground service owns the active-run lifetime, not the Activity or a widget. Activity destruction/recreation must not instantiate a second Rust coordinator. Flutter reconnects to durable state and an event stream when visible. Android service/bridge bootstrap provides a single coordinator with an explicit shutdown path; its exact plugin versus small native-adapter implementation remains a builder decision.

Start service-backed work following an explicit action while the app is visible. Display an ongoing notification with generic progress, an open-app action, and stop. Do not expose message content or tool arguments on the lock screen. Waiting for approval persists state, notifies the user, and stops active service work; approval opens the app and explicitly restarts the run. No lock-screen/background approval action executes a tool. Opening the notification routes to the conversation's pending request. On approval, revalidate request state, arguments, destination, tool schema, current rules, and execution-slot availability; stale requests are rejected rather than executed. Completion/failure also stops service work. Handle notification permission denial with an explanation of reduced visibility, not a claim that it necessarily prevents foreground services.

This is bounded, user-initiated background work, not a permanently running daemon. Android requires an appropriate declared foreground-service type and may impose runtime limits. For example, Android 15+ dataSync foreground services have a six-hour allowance in a 24-hour period under the documented conditions; a foreground service is not an unlimited-execution exemption. Do not choose dataSync merely to evade restrictions, and do not misuse media/location types.

Early implementation feasibility gate: verify the appropriate service type and eligibility for this workload against the chosen Android target and intended distribution; exercise native/service ownership of the Rust loop with the Activity absent. Handle service start refusal and timeout by checkpointing and showing an explicit paused/interrupted state. Never silently downgrade “continues while minimized” to foreground-only behavior. If platform rules prevent the proposed behavior, return that constraint for a user decision before completing the feature.

Force-stop, process death, device reboot, lost networking, and manufacturer battery restrictions can stop work. No automatic boot restart or battery-exemption request in v1. Validate app switching on the user's Android device as well as an emulator; do not claim headless tests prove background behavior.

## Storage, privacy, and history

SQLite owns conversations/messages, profile snapshots, endpoint metadata, tool definitions and rules, run/attempt/approval state, attachment metadata and extracted pages, and artifact references. Use versioned transactional migrations, foreign keys, parameterized queries, and isolated temporary databases in tests. Serialize logical state transitions; never hold a database lock across network waits.

Store original PDFs and generated outputs under app-private, opaque generated IDs, not model-provided paths. Attachments/artifacts have one owning conversation; there is no cross-conversation sharing or deduplication in v1. Attaching the same document elsewhere creates a separately owned copy. Stream imports to bounded staging files, commit attachment references only when ready, and reconcile abandoned staging/orphan files after interruption. Delete a conversation only after its run is stopped; cascade owned rows/files without touching shared endpoint/profile definitions. Removing app data removes local history; a transcript export is not a restorable backup.

Secure storage holds configured credentials separately from SQLite. Support adding, replacing, and deleting credentials without revealing existing secret values in the editor. Missing/invalidated Keystore entries require re-entry, never fallback to plaintext storage. A profile referencing missing credentials remains browsable but cannot dispatch affected requests; show a configuration error with a link to repair it. Endpoint/server deletion is refused while referenced unless the user explicitly removes those references; existing history stays readable. Credential replacement affects future requests, not a request already sent. Keystore-backed protection does not encrypt all conversation data: v1 relies on Android app sandbox/device encryption, not a separately encrypted SQLite database. Exclude private database, attachment, and credential material from automatic cloud backup; explicit user exports are separate actions. No analytics or payload telemetry by default. Diagnostics redact configured secrets and omit message/tool bodies unless the user explicitly chooses to inspect them locally. This cannot guarantee detection of arbitrary secrets present in a document.

Search covers conversation titles and message text, not full attachment contents in the history search UI. Markdown transcript export includes messages and clearly labeled tool summaries; detailed tool payloads and attachment files are opt-in because they may contain sensitive data. Never export configured authentication values. Show a preview/warning before sharing. Browsing stored conversations works offline; inference and remote MCP do not.

## Context and PDF inputs

Parser execution follows the [PDF worker amendment](2026-11-07-pdf-worker-design.md): a disposable non-exported `:pdf_worker` process runs the Rust decoder, while the main process bounds/validates the response and owns persistence. This is same-UID fault containment, not an isolated-UID security sandbox or a guaranteed total native-memory ceiling. Main-process PDF parsing is not an allowed fallback. The amendment's bounded pipe protocol, deadline, process-death handling and additional test gate supersede the original in-process parser assumptions.

Use Android's document picker, copy the selected file into private storage, validate it as a supported PDF rather than trusting its extension, and extract embedded text locally. Preserve physical page numbers starting at 1, filename, attachment ID, and extraction status. Reading order and tables may be imperfect. An empty page does not prove it is scanned; report “no extractable text” rather than inventing a diagnosis. Reject encrypted/password-protected and malformed files clearly in v1. Do not execute embedded scripts, follow embedded links automatically, or use file text as system instructions.

Expose bounded built-in tools for listing this conversation's attachments, reading extracted page ranges, and searching extracted text. This allows a large supported document to be read incrementally without putting its entire content in every prompt. No vector database, embeddings, or cross-conversation document search. The model receives filenames/page summaries and retrieves text as needed. Before the first document-backed request in a conversation, show a notice naming the inference endpoint and explaining that attachment metadata and retrieved text are sent there; changing the inference destination requires renewed acknowledgment. Record retrieved page ranges in inspectable tool history. This is disclosure of inference transmission, not permission for unrelated MCP writes. Built-ins are subject to the same ask/allow/block machinery and default ask rules, with a clear local-tools identity.

Answers about documents should cite filename and physical page number and distinguish observed text from inference. The app can validate that referenced attachment/pages exist, not that an LLM's interpretation is correct. State missing/failed extraction; do not claim images/charts were understood. Full-document summaries that exceed the context budget must disclose partial coverage or offer a separately approved multi-step summary, never silently claim completeness.

Context assembly includes instructions, tool schemas, selected history, selected document excerpts, and reserved output space. Show estimated usage versus a user-configured model context window and actual usage where supplied. Unknown tokenizers use a labeled conservative estimate plus hard byte caps, not a promise of exact fit. If the provider still rejects context length, preserve input and offer explicit shortening/summarization; do not silently drop content. Summarization creates a visible derived context entry, preserves the original transcript, and retains unresolved tool outcomes. It is not a source of fresh server facts.

## Generated files

A restricted built-in tool creates UTF-8 Markdown/plain-text artifacts in app-private storage and associates them with the run/conversation. It is governed by per-tool rules. The model supplies a display filename and content, never a destination path. Sanitize display names, use opaque internal IDs, create a new artifact instead of overwriting an existing one, and record failures rather than claiming a file exists.

Provide a safe text/Markdown preview without executing HTML/scripts or automatically fetching remote images. Saving uses Android's destination picker; sharing uses a content URI with temporary grants, not a private filesystem path. Export/share is a separate user interaction, not automatically triggered by the model. Generated summaries are model-authored outputs, not authoritative verbatim transcripts. PDF generation, arbitrary binaries, and code execution are not required.

## Proposed initial limits

These are builder-proposed defaults for review, not measured phone capacity guarantees. Expose run step/time budgets and model context windows as validated settings; keep the file/result safety caps fixed in v1. Any necessary change to a fixed cap is a documented design adjustment, not silent truncation:

| Boundary | Initial limit / behavior |
| --- | --- |
| Active execution | One run; sequential tool calls |
| Per run | 20 inference requests, 40 tool attempts, 15 minutes cumulative active time |
| Network request | 30-second connection timeout; 120-second response-idle timeout, still bounded by run deadline |
| Waiting for approval | No service held; active-time budget paused |
| PDF input | 10 MiB/file; 500 pages/file; 2 MiB extracted UTF-8 text/file; 5 attachments/conversation |
| Retrieved text / tool result | 32 KiB per document read; 1 MiB raw MCP result |
| Generated output | 1 MiB/file, bounded by available storage |
| Context | Explicit profile/model window; ask before summarizing or omitting selected content |

Enforce limits while reading/streaming, not only after allocating an unbounded response. Oversized MCP results are reported as too large; preserve outcome information and a bounded diagnostic, not a successful empty result. A valid tool response that exceeds the local payload limit is not permission to repeat the action. Budget includes approval denials and failed attempts. Keep byte/page/result safety caps even if configurable run budgets are raised. Global disk quota is a builder-chosen safety cap with a visible cleanup error rather than unexpected deletion of history.

## Verification and acceptance

Use repository `AGENTS.md` verification commands. Documentation-only work does not require app builds. Implementation must add tests at the lowest meaningful layer:

- Rust: profile snapshots/live policy revocation; server/name/schema identity; exact approval binding; blocked/unknown tools; streaming assembly; tool argument validation; bounded loops; deterministic cancellation ordering; SQLite migration/restart recovery; interrupted writes; unknown outcomes; context budgets; file bounds/path traversal; PDF extraction with page citations, empty/mixed pages, malformed/encrypted files, and size limits. Use fake transports/clocks and fixed PDF fixtures; no live credentials or sleeps.
- Flutter widgets: fake-backend profile editing, server/model tests, streaming/errors, tool approvals, stop, interruption notices, history search/delete/export, file import errors, extraction coverage, generated artifacts, and context prompts.
- Real bridge: initialization, errors, event subscription/reconnection, cancellation, and representative persistence round trips through regenerated bindings.
- Android: release build connectivity, foreground-service eligibility/start/timeout, minimized Activity-independent execution, notification stop, pending approval then resume, process kill recovery, Keystore access, document picker and content-URI sharing. Verify no duplicate run after Activity recreation. Test any insecure-HTTP opt-in and backup exclusions. Device tests must not perform real destructive MCP writes; use a controlled fixture server.

Core acceptance scenario: configure an inference endpoint and token-authenticated MCP server; create a profile; converse through multiple tool steps; approve an ask tool, automatically run an allowed tool, and reject a blocked tool; switch apps during a run and return to its saved result; recover from an interrupted write without replaying it; attach a selectable-text PDF and get a page-cited answer; generate/save/share a Markdown summary; reopen the app with history intact.

Before declaring implementation complete, run Rust fmt/clippy/tests, Dart format/analyze/widget tests, and relevant real-bridge/device checks. Report unavailable targets and skipped checks honestly. Regenerate using FRB 2.13.0. No lint weakening or generated-output patches.

## Builder decisions and feasibility work

Choose dependencies only after checking Android ABI support, licensing, maintenance, and safe APIs. Keep async execution off the UI thread and blocking PDF/database work off async executor threads. PDF parser selection and the amendment's separate-process fault-containment policy need explicit adverse-input/process-death tests; parser warnings cannot prove extraction completeness. Neither per-stream caps nor process separation establishes a portable total-memory ceiling; do not claim that guarantee. Resolve supported Android minimum/target SDK from actual selected dependencies and tooling; do not guess numeric SDK versions from Flutter placeholders.

Early gates are foreground-service eligibility/ownership, an Android-compatible embedded-text PDF extractor, and real compatibility of the intended inference model and MCP transport. If a gate fails, report the constraint instead of silently dropping a confirmed requirement or adding a remote hosting service. Package selection and implementation order remain to the implementation plan. Proposed initial delivery is a sideloadable test APK, not Google Play publication. This is a builder proposal for user review, not a distribution preference the user previously stated. Store publication and production signing are separate; never publish, change CI, or treat template debug signing as a production release as a side effect of implementation. The service feasibility gate still applies to sideloaded apps; sideloading does not bypass Android execution restrictions.

## Sources

- Product requirements and architecture approved in the design conversation; numerical defaults and compatibility boundaries are proposals in this draft.
- Repository: `AGENTS.md`, `lib/main.dart`, `rust/src/api/simple.rs`, `rust/Cargo.toml`, `pubspec.yaml`, `flutter_rust_bridge.yaml`, and Android runner files inspected during drafting.
- Android foreground service types: https://developer.android.com/develop/background-work/services/fgs/service-types
- Android foreground service timeouts: https://developer.android.com/develop/background-work/services/fgs/timeout
