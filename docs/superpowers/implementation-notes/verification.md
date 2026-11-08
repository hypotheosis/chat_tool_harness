# Task14 CORE acceptance / installable APK

Authority: `.superpowers/sdd/2026-11-07-android-mcp-agent/task-14-contracts.md` overrides PDF steps in the brief. **Core checkpoint only; PDF and whole-project completion are not claimed.** Final whole-change review remains after PDF integration.

## Artifact / device

- Repository: `$HOME/repos/hypotheosis/chat_tool_harness-agent`.
- Final command: `flutter build apk --debug --target lib/main.dart`, after all fixture tests.
- APK: `$HOME/repos/hypotheosis/chat_tool_harness-agent/build/app/outputs/flutter-apk/app-debug.apk`.
- SHA-256: `d1167dc54447c19b2f41a933c1e56eedbe66564519246858fdc435a34de1e757`.
- Size **226900790 bytes**; package `com.example.chat_tool_harness`, version `1.0.0` / code `1`, min SDK **24**, target/compile SDK **36**.
- Packaged **arm64-v8a, armeabi-v7a, x86_64** each have `libflutter.so` and `librust_lib_chat_tool_harness.so`. ARM64 also has Flutter's `libVkLayer_khronos_validation.so`. i686 Rust was built, but is not a packaged Flutter phone ABI.
- Signature v2 **verified**; `C=US, O=Android, CN=Android Debug`; certificate SHA-256 `7458ff4a7b2b7a2e28b762c75abc02e8c8b42da75f4915ce9e27049b61615f45`. Debuggable/debug-signed, **not production/store release**; signing unchanged.
- Final APK installed successfully; normal MainActivity cold launch `Status: ok`, 1640 ms. No fixture APK built afterward.
- Device: `sdk gphone64 x86 64`, `emulator-5554`, Android **16 / API36**, android-x64; reported ABIs `x86_64,arm64-v8a`.
- Fingerprint: `google/sdk_gphone64_x86_64/emu64xa:16/BE2A.250530.026.F3/13894323:userdebug/dev-keys`.
- Host Ubuntu 24.04.3 LTS, kernel `6.18.56-ogc1.1.fc44.x86_64`. `flutter devices` also listed Linux and Chrome; not used for acceptance. No physical device.

## Coverage matrix

PASS means actually executed here. Workflow config/rules use public backend commands; Send/approval/Stop/artifact/history use public widgets. Not every settings field was driven through Android UI. All fixtures are local/synthetic, with own temporary private storage.

| Requirement | Evidence | Result / boundary |
|---|---|---|
| Local endpoint/server/profile | `agent_workflow_test.dart`: `android_mcp_markdown_workflow`, real FrbBackend + Android service admission | PASS; HTTP explicitly opted in, synthetic resolver callback; no live providers |
| Read → Ask-write → Allow → Block | Workflow observes actual MCP calls/args and persisted states | PASS; read/write default Ask, writes 0 before approval, 1 after Ask, 2 after Allow; Block denied without a third write |
| Exact immutable approval | Workflow changes approval args only, requires rejection/no write; UI approves genuine binding | PASS; exact `ask-exact` dispatched; existing Core/bridge tests cover additional identity/schema boundaries |
| Streaming / Stop | Workflow holds unbuffered SSE, backend + chat display partial, keyed Stop | PASS; Cancelled with partial retained; no tool call |
| Sent-write unknown / no replay | Workflow holds MCP response after effect, Stop, then rejected Resume/no request | PASS; workflow total writes **3** includes two previous successful writes, distinct from restart counter |
| Background / same handle | `background_lifecycle_test.dart` | PASS; real FGS, Activity finish/restore, same runtime/backend handle, approval releases service; actual service loss → Unknown; stale-generation actions reject |
| Real process restart / counter 1 | `python3 test_driver/task12_process_recovery.py` | PASS; PIDs **15130 → 15220 → 15306**, runtime changes; pending Open restart sends no request, actual foreground tap permits one write; sent-write death recovers Unknown/no replay, writes **1** |
| Generated Markdown | Workflow inference calls local `create_artifact`, real approval/store/FilePanel | PASS; `summary.md` literal preview; no external Save/Share |
| Offline history / rename / search / export / reopen | Workflow closes fixture listener, uses HistoryScreen; `history_bridge_test.dart` also closes/reopens real repository | PASS; private export preview/generation, not external delivery; new offline turn fails honestly and retains messages/artifact |
| Credential privacy | Workflow checks transcript/export/notification snapshot; owned PID **15794** logcat + workflow output scanned for synthetic key | PASS for collected evidence: **443 logcat lines**; not universal log retention or memory-erasure proof |
| Native credentials | `credential_lifecycle_test.dart`, `CredentialStoreTest` | PASS; real replace/delete/own-alias invalidation/reentry, redaction/backup boundaries; workflow itself uses synthetic callback |
| URI / Intent / native copy | `file_access_test.dart`, `ArtifactDocumentsTest`, `DocumentAccessTest` | PASS; own-file URI reads/read-only share flags, Intent construction, controlled copy/cancel/failure/late-open closure; **not external picker/receiver delivery** |
| Notifications / native refusal | Background suite, selected AgentService tests, isolated denied-permission test | PASS; generic payload, stale Stop, denied notifications still admit FGS; real no-resumed-Activity guard refusal, not actual OS FGS exception/quota delivery |
| Generated bridge | FRB **2.13.0**, pre/post SHA-256 | PASS; all **18** generated files byte-identical |
| Normal APK / ARM64 | Final build + ZIP/aapt/apksigner + install/launch | PASS on x86_64 emulator; ARM64 libs packaged, **not physically executed** |
| PDF scenario / worker | Excluded by core contract | NOT ACCEPTED; no PDF integration scenario, `pdf_worker_test.dart`, PDF release scenario or PDF native instrumentation invoked |
| Final whole-change review | Deferred by contract | NOT PERFORMED; this package is Task14 core only |

## Exact commands / results

All commands from repository root unless `cd android` shown. Scoped Android environment:

```sh
export JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64
export PATH="$JAVA_HOME/bin:$HOME/repos/flutter/bin/cache/dart-sdk/bin:$HOME/repos/flutter/bin:$HOME/Android/Sdk/platform-tools:$PATH"
export GRADLE_OPTS=-Dorg.gradle.java.home=$JAVA_HOME
```

Gradle **8.14**, Launcher JVM **17.0.20.1**, daemon `/usr/lib/jvm/java-17-openjdk-amd64`. `rustup target list --installed` confirmed existing aarch64-linux-android, armv7-linux-androideabi, i686-linux-android, x86_64-linux-android; nothing installed.

Logs below are in `W=.superpowers/sdd/2026-11-07-android-mcp-agent`.

| Actual command | Outcome | Log |
|---|---|---|
| `flutter_rust_bridge_codegen --version`; `flutter_rust_bridge_codegen generate` | 2.13.0; exit 0 | `task-14-codegen.log`, console version |
| `sha256sum rust/src/frb_generated.rs $(git ls-files --cached --others --exclude-standard lib/src/rust)` saved before generation; `sha256sum -c "$W/task-14-generated-before.sha256"` | all 18 OK | `task-14-generated-{before.sha256,check.log}` |
| `cargo fmt --manifest-path rust/Cargo.toml -- --check` | exit 0 | `task-14-fmt.log` |
| `cargo clippy --manifest-path rust/Cargo.toml --all-targets -- -D warnings` | exit 0 | `task-14-clippy.log` |
| `cargo test --manifest-path rust/Cargo.toml` | **253 passed**, 0 failed/ignored, exit 0 | `task-14-rust.log` |
| `dart format --output=none --set-exit-if-changed lib test integration_test` | 70 files, 0 changed, exit 0 | `task-14-format.log` |
| `flutter analyze` | no issues, exit 0 | `task-14-analyze.log` |
| `flutter test` | **75 passed**, exit 0 | `task-14-flutter.log` |
| `flutter devices` | emulator-5554/API36, Linux, Chrome | console |
| `flutter test integration_test/simple_test.dart integration_test/agent_bridge_test.dart integration_test/background_lifecycle_test.dart integration_test/file_access_test.dart integration_test/credential_lifecycle_test.dart integration_test/history_bridge_test.dart integration_test/agent_workflow_test.dart -d emulator-5554` | **13 passed**, exit 0 | `task-14-integrations.log` |
| `flutter test integration_test/agent_workflow_test.dart -d emulator-5554` final tightened assertions/PID marker | **1 passed**, exit 0 | `task-14-workflow-final.log` |
| `cd android && ./gradlew --version && ./gradlew app:testDebugUnitTest app:connectedDebugAndroidTest -Ptarget=lib/main.dart -Pandroid.testInstrumentationRunnerArguments.class=com.example.chat_tool_harness.CredentialStoreTest,com.example.chat_tool_harness.ArtifactDocumentsTest,com.example.chat_tool_harness.DocumentAccessTest,com.example.chat_tool_harness.AgentServiceTest#notification_contains_no_payload_and_stale_notification_cannot_execute` | **12 instrumentation passed**, exit 0; JVM unit task **NO-SOURCE**, not a passed JVM suite | `task-14-native.log` |
| `python3 test_driver/task12_process_recovery.py` | real death/restart PASS, writes 1, exit 0 | `task-14-process.log` |
| `adb -s emulator-5554 install -r build/app/outputs/apk/debug/app-debug.apk`; `adb -s emulator-5554 install -r build/app/outputs/apk/androidTest/debug/app-debug-androidTest.apk` | Success | `task-14-denied.log` |
| `adb -s emulator-5554 shell pm revoke com.example.chat_tool_harness android.permission.POST_NOTIFICATIONS` | exit 0, own test package only | `task-14-denied.log` |
| `adb -s emulator-5554 shell am instrument -w -e class com.example.chat_tool_harness.AgentServiceTest#notification_denial_reduces_visibility_but_foreground_start_acknowledges com.example.chat_tool_harness.test/androidx.test.runner.AndroidJUnitRunner` | **1 passed**; test restores permission | `task-14-denied.log` |
| `adb -s emulator-5554 logcat -d --pid=15794` | 443 owned-process lines; exact synthetic key absent in logcat + workflow output | `task-14-owned-logcat.log`, `task-14-log-privacy.log` |
| `flutter build apk --debug --target lib/main.dart` | exit 0, final normal universal APK | `task-14-apk.log` |
| `sha256sum build/app/outputs/flutter-apk/app-debug.apk`; `stat -c '%s bytes' build/app/outputs/flutter-apk/app-debug.apk` | hash/size above | `task-14-apk-inspection.log` |
| `$HOME/Android/Sdk/build-tools/36.1.0/aapt dump badging build/app/outputs/flutter-apk/app-debug.apk` | ABIs/package/SDK above | `task-14-apk-inspection.log` |
| `$HOME/Android/Sdk/build-tools/36.1.0/aapt dump xmltree build/app/outputs/flutter-apk/app-debug.apk AndroidManifest.xml` | packaged manifest inspected | `task-14-apk-inspection.log` |
| `$HOME/Android/Sdk/build-tools/36.1.0/apksigner verify --verbose --print-certs build/app/outputs/flutter-apk/app-debug.apk` | Verifies / Android Debug certificate | `task-14-apk-inspection.log` |
| Python `zipfile.ZipFile` inventory/assertions | required ARM64/ARMv7/x86_64 Flutter+Rust entries present; kernel lacks `TASK14_OWNED_PID=` and `TASK12_SCENARIO` fixture markers | `task-14-apk-inspection.log` |
| `adb -s emulator-5554 install -r build/app/outputs/flutter-apk/app-debug.apk`; `adb -s emulator-5554 shell am start -W -n com.example.chat_tool_harness/.MainActivity` | Success / Status ok | `task-14-apk-inspection.log` |
| `git diff --check`; `diff -qr` baseline/live `rust/src`, `android/app/src`, `lib`, `integration_test` | clean whitespace; production/bridge trees unchanged; only new workflow test | console / scoped package |

Process driver internally builds `integration_test/process_recovery_scenario.dart` with unique `TASK12_SCENARIO` and local `TASK12_FIXTURE`, temporarily reverses its ephemeral port, kills only its own main PID, cleans its UUID directory, and rebuilds/reinstalls `lib/main.dart`. No app-data clear or emulator restart. Ordinary full Rust/Flutter regression suites include existing PDF headless tests; that is not PDF device acceptance.

## Boundary inspection

- Main manifest declares release-inherited INTERNET, FOREGROUND_SERVICE, FOREGROUND_SERVICE_DATA_SYNC, POST_NOTIFICATIONS. AgentService is non-exported `dataSync`; final packaged manifest confirms. No release manifest override; no release APK built/certified.
- `allowBackup=false`; backup/extraction XML excludes private root/files/database/preferences/external and device-protected equivalents from cloud/device transfer. Native backup test passed; no actual backup experiment claimed.
- FileProvider non-exported, URI grants enabled, only `agent/artifacts/` path. DocumentAccess checks canonical UUID-owned text files/rejects symlinks; shares read-only with ClipData; explicit picker callback copies off-main with lifecycle-owned closure. URI/copy tests are not external delivery evidence.
- MainActivity borrows application-owned engine and closes document host on destruction. Real coordinator lifecycle harness retained; no `FakeNativeStep`/obsolete prototype production wiring found, so no removal warranted.
- AgentProbe is `src/debug`; release DebugProbeInstaller is no-op. Debug APK intentionally includes debug lifecycle probes, not release-hardening evidence. Final compiled Dart kernel lacks fixture-entrypoint markers.
- Baseline comparison: no production Rust/Dart/Android/generated changes. Task14 adds acceptance test and documentation/package only.

## Failed attempts / unpassed paths

- Exploratory workflow failures were test issues: wrong enum spelling, async/lazy-list readiness, keyboard obstruction, unscoped scrollable, stale discovery observations after transport close. Fixture SSE needed unbuffered output. Tests now use fresh public discovery; no permission shortcut or production fix.
- Initial analyze found one test-fixture missing brace; fixed. Initial log guard incorrectly expected test print in native logcat: Flutter captured it in test output. Corrected guard identifies actual PID/native Flutter entries and scans both outputs; final checks pass. No production RED/GREEN fix claimed.
- Existing JVM native-access/cgroup/Gradle deprecation warnings remain; emulator-console warning did not prevent 12 native tests executing. No lint weakening/upgrades/toolchain/license/signing changes, external save, helper agents, Git/CI/catalog writes or emulator restart.
- **Manual/not passed:** external SAF destination choice/provider write/reopen; share receiver consumption/read-grant lifetime; actual OS quota timeout/FGS-start-exception delivery; physical ARM64 execution; OEM battery/lifetime; lock-screen rendering; pre-API36 Android. Denied-notification and Activity-guard tests do not replace OS-delivery evidence.
- **Deferred:** PDF attachment/page/citation/worker scenarios and one final whole-change review including PDF. Task13 optional busy-refresh coalescing and incremental export construction remain untouched for final review.
