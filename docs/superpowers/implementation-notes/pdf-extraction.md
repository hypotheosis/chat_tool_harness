# PDF extraction feasibility — worker revision and phase 2B evidence

Phase 2B added a private same-UID `:pdf_worker` service, FD/pipe transport, bounded Kotlin host reader, and headless Dart/FRB entrypoint. The deferred worker gate now has API-36 x86_64 emulator evidence: deterministic blocked start/callback deadlines and cleanup, EOF/death ordering at Binder observation, partial cancellation, disconnect/no replay, exact frame/wire bounds, wrong-PID control, actual callback-Binder death, debug extraction/abort/real 60-second timeout, and normal `lib/main.dart` release/AOT worker extraction/cancel/confirmed death/explicit retry. See the [final gate report](../../../.superpowers/sdd/2026-11-07-android-mcp-agent/pdf-gate-final-report.md) for exact commands, failed attempts, and limits. API 24 and physical-device acceptance are **not passed**. This proves neither a parser-wide RAM ceiling nor a permission sandbox; attachments UI/storage/tools and whole-feature acceptance remain deferred.

## Original investigation (retained below)


The user approved revising PDF support around a separate Android process. The detailed [PDF worker amendment](../specs/2026-11-07-pdf-worker-design.md) and revised Task 2 were accepted when the user asked to continue; worker/parser implementation is now authorized but not yet verified. The source investigation below explains why the original in-process approach was blocked.

Task 2 has not added a PDF adapter. The proposed lopdf 0.47.0 is available (`cargo search lopdf --limit 1`, `cargo info lopdf@0.47.0`) and MIT-licensed, requires Rust 1.88 (host 1.99.1), and offers a per-stream decompression bound, but the public API does not provide a parser-wide memory budget or Android-process isolation.

Source inspected in the downloaded crate at `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/lopdf-0.47.0`:

- `src/load_options.rs:41-61,73-81,114-122`: `LoadOptions::max_decompressed_size` bounds **each** eager object/xref stream; the default is 256 MiB. No total decoded-memory or object-count bound is exposed.
- `src/reader.rs:755-817,1017-1103`: `Document::load_mem_with_options` resolves xrefs and loads the entire object graph before the caller can inspect pages; each object stream can decode independently up to the limit and parsed objects are retained in `Document::objects`. A 10 MiB input can contain multiple small compressed streams, so 4 MiB per stream is not a 4 MiB total memory cap.
- `src/object_stream.rs:62-128`: `new_with_limit` caps one decoded object stream, then parses and stores its members in a map. `src/parser/mod.rs:175-196,235-278,286-335` allocates name/string/array nodes from untrusted content with nesting limits but no shared allocation budget.
- `src/parser_aux.rs:70-89,112-153,175-231,375-392`: `extract_text_with_limit` bounds page-content and font decompression but accumulates extracted text in an unbounded `String`; text-chunk intermediate allocations also occur. Its limit is **not** an extracted UTF-8 cap or parser-wide bound.
- `src/document.rs:569-575,1017-1105`: page iteration has a depth/object traversal guard, but that is post-load and not a parser allocation budget.

These source paths are reproducible with `cargo info lopdf@0.47.0` followed by the indicated ranges. A cooperative worker thread, `catch_unwind`, or timeout does not enforce a process-wide memory ceiling or contain an OOM abort on Android. The selected revision proposes a disposable same-UID `:pdf_worker` with bounded host protocol/output, a request deadline, independent worker self-termination and confirmed-death handling. It is fault containment, not an isolated-UID permission sandbox or a strict total-memory ceiling; unconfirmed termination disables further extraction rather than being reported as stopped. The detailed amendment was accepted and this gate may resume implementation under it. No remote parsing, OCR, passwords, or new OS package is proposed.

No source/dependency/fixture changes were made; no RED→GREEN or APK claims are made. The page/file/text cap tests cannot establish the security gate while parsing itself is uncontained. Task 1 files and the preexisting lockfile changes were left untouched.
