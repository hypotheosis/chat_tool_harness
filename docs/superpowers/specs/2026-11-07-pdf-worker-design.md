# PDF worker revision — separate-process fault containment

Status: the user approved this detailed amendment by asking to continue after its review handoff. Task 2 implementation is authorized under the existing subagent-driven workflow. This contract does not itself describe implemented or verified worker behavior. It amends the [v1 design](2026-11-07-android-mcp-agent-design.md) and Task 2 of the [implementation plan](../plans/2026-11-07-android-mcp-agent.md). Tasks 3–14 remain unimplemented; the Android runtime feasibility gate passed separately.

## Intent and unchanged scope

Read selectable-text PDFs locally without running the parser in the chat/agent process. Keep Rust responsible for decoding, extraction policy, validation, and page records; Kotlin supplies Android process/descriptor lifecycle, and Dart supplies a minimal FRB entrypoint. Preserve the existing minimum SDK 24, target SDK 36, Rust edition, and FRB 2.13.0. Keep the confirmed 10 MiB input, 500 pages, 2 MiB extracted UTF-8 text, five attachments per conversation, and no OCR, visual interpretation, passwords, or remote parsing.

The library investigation found that lopdf 0.47.0 can cap individual decompression operations but has no aggregate parser allocation budget. Testing a few valid PDFs cannot prove a global memory ceiling. This revision replaces the in-process safety claim with an explicit, testable fault-containment policy rather than pretending a worker thread or a post-allocation size check supplies that ceiling.

## Approaches and decision

### Selected proposal: private same-UID process

Use a non-exported, explicitly bound Android service in `android:process=":pdf_worker"`, with one minimal headless Flutter engine/FRB runtime for one job. Parse with the candidate Rust/lopdf adapter only inside that process. No handwritten JNI or FFI is required. Terminate the worker after each job; no parser daemon or cross-job cache.

This is **reliability isolation, not a security sandbox**. Do not set `isolatedProcess=true` while assuming a Flutter engine still has normal app cache/storage access. The worker retains the app UID and permissions, so a fully compromised native worker is not prevented from accessing same-UID files or networking. The application does not send it credentials, database handles, conversation content, MCP configuration, or inference clients, but this is least-data design, not an OS-enforced access boundary.

A process-local parser crash or abort can be observed through binder death without directly aborting the main process. The worker still contributes to system memory pressure; Android may kill either process. There is **no portable hard resident-memory budget or guarantee that chat can never be affected**. If true untrusted-code permission isolation or a kernel-enforced total memory cap becomes a requirement, this design is insufficient and needs another decision.

### Not selected: isolated UID plus native/Java decoder

`isolatedProcess=true` provides a different UID with no normal app permissions and can receive a specific open file descriptor through Binder. However, it is not an established supported environment for a Flutter engine relying on app cache/data paths. A small Java/platform decoder or a separately reviewed native bridge would change dependencies and the implementation architecture. Framework `PdfRenderer.Page.getTextContents()` requires API 35 and cannot silently replace API-24-compatible PDF support. A Java PDF port would need its own maintenance/security review, not an assumption that Java establishes bounded parsing.

### Rejected: main-process parsing with a timeout

A cooperative timeout, `catch_unwind`, or a worker thread cannot undo native allocations or contain a process abort. A per-stream bound is still useful inside the separate worker but is not an aggregate heap budget.

## Process bootstrap and ownership

`AgentApplication.onCreate()` currently starts the cached chat engine eagerly. Add a process-role check **before** engine initialization. The main process keeps the existing engine/runtime behavior. `:pdf_worker` initializes neither `AgentRuntime` nor chat Dart `main`; any unexpected/unknown process role fails closed rather than starting another coordinator. Use public process-name APIs (API 28+ `Application.getProcessName`, with an API-24-compatible own-process fallback); no hidden-API reflection.

`PdfWorkerService` lazily initializes a worker-specific `FlutterEngine` only after accepting one valid job. Disable automatic unrelated plugin registration. Execute a retained AOT entrypoint `pdfWorkerMain` marked with `@pragma('vm:entry-point')`; it initializes FRB and worker channels, not widgets, databases, profiles, network clients, or a second coordinator. Do not reuse `agent_runtime` as the worker engine cache key. Process memory/static state is independent even though the UID is shared.

A normal bound service is enough; it is not another foreground service and does not create another notification. Bind only while an explicit import/agent attachment operation is active. Binding does not grant unlimited background execution or exempt work from Android lifecycle policy. Main-process cancellation, app shutdown, and client disconnect end the job. The parent may be a visible Activity-owned operation or the already-authorized active agent service, never an automatic boot task.

## Request and IPC contract

One active PDF extraction application-wide. A concurrent request returns Busy; no hidden queue or parallel parser fleet. A request contains:

- random request ID and protocol version `1`;
- read-only `ParcelFileDescriptor` for the stable app-private input snapshot;
- a write-only pipe descriptor for the response;
- fixed limits, not prompts, model-generated paths, or optional credentials.

`PdfWorkerRequest.snapshot_path` is a **main-process-only** handoff from Rust staging to the trusted Kotlin broker. It never crosses Binder into the worker and is never sent to the worker FRB function. The broker resolves it only under the app-owned staging root, opens it read-only, then transfers descriptors. Worker FRB sees only its internally constructed descriptor paths. The parent opens/checks the regular snapshot file, including the 10 MiB cap, before binding. The worker rechecks input length/read bounds. Files are not modified after publication. The worker owns transferred descriptor duplicates until extraction ends; the parent closes its unused pipe ends immediately. No full PDF or extracted document travels in a Binder transaction. Service `exported=false` and explicit binding prevent unrelated apps from invoking this interface.

The small control interface is explicit: `IPdfWorker.start(requestId, generation, inputFd, outputFd, maxInputBytes, maxPages, maxTextBytes, maxDecompressedBytes, IPdfWorkerEvents)`, `acknowledge(requestId, generation)`, and `cancel(requestId, generation)`. IDs are strings, generation is a nonnegative 64-bit integer, descriptors are `ParcelFileDescriptor`, and the four fixed limits are 32-bit integers. The synchronous boolean callback `IPdfWorkerEvents.started(requestId, generation)` runs before parsing; a false response rejects/ends the session. The callback captures identity and returns promptly, without synchronously calling back into the worker control binder. Worker control methods queue engine/parser work and return; parsing never blocks the Binder control thread. In that callback, the host records `Binder.getCallingUid()` and `Binder.getCallingPid()` on that Binder callback thread, requires its own app UID, a PID different from the chat PID, the current outstanding request/generation, and the live control binder obtained from the explicit binding. Link binder death before starting. Do not use a one-way callback whose caller PID may be unavailable. Worker PID is diagnostic only, never an instruction to kill a process. The parent stores the control binder with its job generation; acknowledge/cancel must match that exact session. The worker rejects stale IDs and never starts work on a bind/reconnect alone.

Kotlin can pass its live descriptor numbers to the worker Dart entrypoint as generated internal `/proc/self/fd/<decimal>` paths. Safe Rust file open operations then duplicate the descriptors without handwritten raw-FD ownership or JNI. Descriptor lifetime remains pinned until Rust finishes. **This is a feasibility item to test on Android, not an assumed API guarantee.** Paths are constructed internally, never accepted as model arguments; no general path-opening API is added to tools. If safe FD duplication is unavailable on supported targets, stop for review rather than adding unsafe FFI or opening arbitrary model paths.

### Response framing

Use a pipe protocol, not arbitrary large Binder parcels:

1. Four-byte unsigned little-endian frame length, checked before allocating; maximum 64 KiB/frame.
2. UTF-8 JSON object with protocol version, UUID request ID (36 ASCII characters), nonnegative bounded record sequence, record kind, and bounded scalar fields. Parse incrementally with strict UTF-8 and a flat schema: one top-level object, known scalar keys only, no nested objects/arrays, no duplicate keys, no trailing tokens, no unbounded numeric conversion. Reject nesting before descending rather than constructing an arbitrary recursive JSON tree. This is required so a corrupted worker response cannot turn a 64 KiB frame into a host stack-overflow attack. Kinds are `begin`, `page_begin`, `text_chunk`, `page_end`, `done`, and `error`.
3. `text_chunk` contains at most 8 KiB decoded UTF-8 text, split on character boundaries. This allows JSON escaping within the frame cap. `page_begin/page_end` carry a physical 1-based page number and status (`text`, `no_extractable_text`, or `failed`). No filenames or unrelated metadata are needed in worker output.
4. `done` carries page count and coverage status. Worker Rust must return and its Kotlin owner must close the write-side descriptor after the terminal record; the parent requires EOF with no extra records before treating transport output as validated. Exactly one terminal record is accepted. An `error`, bad frame, premature EOF, sequence gap, duplicate/out-of-order page, wrong request/version, or worker death before valid `done` fails the job.

Bound cumulative decoded text to 2 MiB, page count to 500, and cumulative framed wire bytes to 16 MiB (JSON escaping can expand text). Validate limits while reading, not after buffering the stream. The host reader uses a dedicated executor and holds at most one bounded frame plus the bounded accepted page data. It does not retain 16 MiB of raw protocol. Transport checks bound native/Dart input sizes before forwarding; main Rust validates page structure, coverage, and extracted-text limits again at its owning result boundary. The success payload crossing the main Dart→Rust callback is a generated typed `PdfWorkerResult { request_id: String, extraction: PdfExtraction }`: at most 500 `PageText` records, an enum status per page, and cumulative UTF-8 text at most 2 MiB. No raw JSON frames, descriptors, worker PID, filenames, arbitrary metadata maps, or full PDF accompany it. Failure uses a typed `PdfFailure` code, not an arbitrary worker exception string.

Kotlin retains only accepted bounded page text and one frame, releases each parsed frame immediately, and drops its page builders after sending the bounded method-channel result. Dart maps that result once into generated DTOs and releases the intermediate map after the FRB callback; Rust owns the accepted extraction until commit/rejection. Framework codecs may make additional bounded copies and UTF-16 storage can exceed the UTF-8 count. Do not claim an exact total host heap cap: the enforceable limits are per-frame/wire/page/text sizes and one result in flight, not a numeric RAM ceiling across Flutter/FRB. Never accumulate completed jobs or raw frame histories. Tests measure/check representation sizes and cleanup, not a fabricated precise heap guarantee.

No durable attachment success is published until the complete response passes validation. Cancellation ordering is explicit: host requests have serialized Pending → Receiving → Validated or Failed/Cancelled transitions; a cancel observed before Validated wins even if a `done` frame has arrived. After Validated, native success is only provisional data, not an attachment. In Task 10 the coordinator owns an import token keyed by request ID and serializes cancel versus the repository commit: cancellation recorded before commit rejects/cleans the provisional result; a transaction committed first returns AlreadyCompleted for a later cancel, without deleting or pretending to roll back the attachment. The commit/cancel linearization point is the main repository transition, never timing on the worker pipe. A successful but incomplete extraction may include explicit failed-page statuses; never imply those pages were read. A fatal/truncated protocol has no successful partial-document result. Input files/staging cleanup remain owned by the main process and Task 10, never by a child-supplied pathname.

## Resource policy, termination, and recovery

- Maintain parser-level 4 MiB per-stream/page decompression bounds where lopdf exposes them. Check extracted output between pages/chunks; a parser may still allocate more internally before it returns, which is why it is separated from chat.
- Use a 60-second monotonic **wall-clock request deadline** covering binding, engine startup, parsing and output, not a claim about CPU seconds. A parent watchdog enforces the request outcome independently of worker progress. This is not a hard real-time/kernel CPU guarantee.
- Cancel closes/abandons the response and requests termination of only the currently bound worker session. A dead child yields `PdfFailure::WorkerDied`; timeout yields `PdfFailure::Timeout`; invalid framing/bounds yields `PdfFailure::Protocol`/`PdfFailure::Limit`. Preserve the original input for explicit user retry or removal; do not silently retry/reparse.
- The parent tracks the live control binder, request ID and generation. Ignore stale completion/cancellation messages. **The parent never sends a PID-based kill.** `acknowledge` after complete host transport validation/EOF asks the responsive worker to exit; it does not mean the attachment has been committed. `cancel` asks it to exit without success. The worker's control thread and an independent worker watchdog call `Process.killProcess(Process.myPid())` only after confirming the worker process role; neither trusts a caller-supplied PID. Start the worker watchdog when the request is accepted, before creating the worker engine or entering blocking decoding. This avoids stale-PID reuse attacks and does not require a broad process-name kill. If worker control/watchdog cannot run, the parent can fail the request but cannot promise immediate CPU termination; the cleanup rule below returns Unavailable rather than hiding that limitation.
- Require worker death/termination confirmation before accepting the next extraction. After a five-second cleanup grace period without confirmed death, report extractor unavailable and refuse new jobs; do not launch more workers or pretend background CPU stopped. Closing/unbinding a service alone is not proof that a parsing thread stopped.
- Binder disconnection releases the parent binding and invalidates the job immediately. Android may recreate a bound service; the parent never resends an old request on reconnection and the worker never auto-starts extraction merely on bind. No request redelivery/retry after process death.
- Request self-termination of the disposable worker after success/failure as well as cancellation and confirm binder death; avoid cached engines/native heaps accumulating across imports. Preserve the chat engine/runtime ID throughout.

Do not apply guessed `RLIMIT_AS` thresholds to a Flutter/Dart VM: virtual-address reservations are not resident RAM and differ by ABI/runtime. Do not claim `largeHeap=false`, JVM heap size, RSS sampling, or Android LMKD constitutes a strict native parser allocation budget. Retain normal OS security settings; no seccomp/SELinux changes, device-wide limits, root tools, or new system packages.

## Contracts for later tasks

Task 2 introduces `PdfLimits`, `PageText`, `PdfExtraction`, and sanitized PDF failure variants in the shared Rust model/error modules. Decoder tests can call the worker decoder directly on controlled fixtures; production main-process imports must use `PdfWorkerHost` rather than calling lopdf.

The worker-only bridge function is async `extract_pdf_worker(input_fd_path: String, output_fd_path: String, limits: PdfLimits, request_id: String) -> AppResult<()>`. It opens only internally supplied descriptor paths, runs blocking decoding off the worker event thread, streams bounded protocol records, and has no coordinator/database/network dependencies. Change `rust/src/api/pdf.rs`/module configuration and regenerate with FRB 2.13.0; do not patch generated files.

Main platform API: `PdfWorkerHost.extract(PdfWorkerRequest) -> Future<PdfWorkerReply>` and `cancel(requestId) -> Future<PlatformReply>`. `PdfWorkerReply::{Extracted(PdfWorkerResult), Failed(PdfFailure)}` carries the bounded typed representation defined above, not a raw path/result parcel. Path comes from app-owned staging, not chat/model text. In Task 2, the main Dart `PdfWorkerHost` is an app-owned adapter over the Kotlin broker, instantiated only in the main process; the worker Dart entrypoint is separate. Tests can call that adapter directly before a coordinator exists. In Task 9, a concrete `PdfWorkerClient` wraps the FRB-captured async `extract_pdf`/`cancel_pdf` callbacks and is injected once into main `RuntimeServices`; the instance owns in-flight request IDs and shutdown cancellation. Task 10's Rust files/import operation receives this client through `RuntimeServices`/the concrete `LocalFileTools` adapter, never imports Android/Dart APIs directly, and applies the cancellation-token/commit rule before committing attachment rows. Main cancellation returns `ImportCancelResult::{Cancelled, AlreadyCompleted, Unavailable}` according to that serialized transition; Task 10 defines this type and `cancel_import(request_id)` alongside import. No global hidden extractor or coordinator is initialized in the child. User credentials and `AgentHandle` never cross into the worker.

## Acceptance and early feasibility tests

Before calling revised Task 2 complete:

1. Controlled Rust fixtures cover selectable text with physical pages `[1, 2]`, empty/mixed pages, encrypted/malformed files, fake extension, decompression refusal, and equality/one-over limits. No hostile OOM stress in the headless test process.
2. Pure transport/model tests cover oversized length prefixes, bounded incremental reads, invalid UTF-8/JSON, wrong IDs/version, page/chunk order, truncated output, 2 MiB text/16 MiB wire limits, cancellation, stale replies, death and cleanup outcomes using fake descriptors/transports where possible.
3. A real Android worker has a PID distinct from chat, reports its worker role, and starts no chat runtime. Test debug and AOT/release entrypoint retention and safe FD transport. API 24 compatibility needs its own supported-target evidence; API 36 alone cannot prove it.
4. Debug/test-only worker fixtures deliberately abort/terminate that worker or withhold output. The main engine remains responsive with the same runtime ID, receives controlled failure, and can start a fresh **explicit** job after confirmed child death. Tests must not exhaust device-wide RAM. Such fixtures prove process-death handling, not a true memory ceiling or all real OOM scenarios.
5. With responsive control/watchdog, cancellation/deadline confirms worker death and closes descriptors without retry; a service rebind never reruns the failed request. A separate simulated missing-death acknowledgment returns Unavailable and forbids a new worker, without falsely claiming CPU stopped. Shutdown is checked even when output is partially written.
6. Re-run the existing Android runtime and native bridge smoke tests, the Rust fmt/clippy/tests, Dart format/analyze/headless tests (adding relevant tests under `test/`), worker integration tests and debug/release APK builds. Report unavailable devices, skipped checks, and default debug signing honestly.

If two-engine/process-role bootstrap, safe FD transport, abort containment, or bounded host decoding fails, report the actual gate failure before downstream work. A fallback to true isolated UID, another parser, a higher minimum Android version, or handwritten native integration requires another reviewed design decision.

## Sources and unresolved builder work

- Local parser investigation: `docs/superpowers/implementation-notes/pdf-extraction.md` and `.superpowers/sdd/2026-11-07-android-mcp-agent/task-2-report.md`.
- Android service process/export/isolated-UID semantics: https://developer.android.com/guide/topics/manifest/service-element
- Bound-service lifecycle: https://developer.android.com/develop/background-work/services/bound-services
- Descriptor transfer: https://developer.android.com/reference/android/os/ParcelFileDescriptor
- Framework PDF text API level: https://developer.android.com/reference/android/graphics/pdf/PdfRenderer.Page
- Process/resource-limit API reference: https://developer.android.com/reference/android/system/Os

Builder selects the smallest Binder interface/helper arrangement, exact Android-compatible dependencies, and test instrumentation from existing conventions. No parser ABI, Flutter worker bootstrap, native-memory cap, or fault-containment success is established by writing this revision.
