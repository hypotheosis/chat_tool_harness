# Agent instructions

## Goal

Build a Flutter UI backed by clean, safe Rust. Prove behavior with automated,
headless tests wherever possible; use an emulator or phone only for behavior
that cannot be meaningfully checked at a lower layer.

## Repository map

- `lib/`: Dart presentation and application wiring; entry point `lib/main.dart`.
- `rust/src/`: Rust backend; bridge-facing functions live in `api/`.
- `flutter_rust_bridge.yaml`: bridge configuration (`crate::api` → `lib/src/rust/`).
- `test/`: headless Dart unit and Flutter widget tests.
- `integration_test/`: tests exercising the running Flutter app and native bridge.
- `rust_builder/`: generated native-build glue, including vendored Cargokit.
- Platform directories contain Flutter runners and platform configuration.

The starting app is the FRB greeting example. Its existing test is
`integration_test/simple_test.dart`; it is not a headless backend test suite.
Add tests alongside new behavior rather than relying on that smoke test.

## Architecture and boundaries

- Put business rules, validation, transformations, and backend state transitions
  in Rust. Dart owns widgets, presentation state, navigation, and user interaction.
- Keep `rust/src/api/` a thin adapter: translate bridge inputs, invoke backend
  operations, and return bridge-compatible values or errors. Extract substantive
  logic into ordinary Rust modules as it grows; do not build a framework upfront.
- Core logic must be callable from Rust tests without Flutter, a device, bridge
  initialization, or platform services.
- Separate deterministic decisions from I/O. Inject clocks, randomness, network,
  storage, and platform access at the boundary when needed for repeatable tests.
  Introduce traits or other abstractions only for a concrete seam.
- Keep UI logic testable with a small app-owned backend interface and fakes when
  needed. Do not make ordinary widget tests depend on loading the native library.
- Validate external input at the owning boundary. Preserve invariants with types
  and constructors rather than repeatedly revalidating trusted internal values.

## Rust safety and maintainability

- Prefer safe, idiomatic Rust and explicit ownership. Avoid unnecessary clones,
  global mutable state, and synchronization; keep locks short and out of waits.
- Return `Result` for recoverable failures. Do not use `unwrap`, `expect`, panic,
  or indexing assumptions on untrusted input in production paths. Test code may
  use assertions and `expect` to make failures clear.
- Keep errors actionable across the bridge without exposing secrets or sensitive
  payloads. Do not swallow errors or replace failure with a misleading success.
- Do not introduce handwritten `unsafe` or FFI unless the task requires it and
  safe alternatives are insufficient. Isolate it, document safety invariants,
  and test the safe wrapper. Generated bridge internals are not a style example.
- Do not copy the demo's `#[frb(sync)]` choice into slow operations. Synchronous
  calls must be short and nonblocking; use the bridge's appropriate asynchronous
  execution path for I/O or expensive work. Do not block an async executor.
- For concurrent work, define cancellation, shutdown, and resource ownership;
  test relevant ordering and failure behavior without timing-dependent sleeps.
- Match existing conventions and dependencies. Do not change the Rust edition,
  add a runtime, or introduce libraries merely to enforce a preferred style.

## Testing strategy

Use the lowest layer that can prove the behavior:

1. **Rust unit tests:** test rules and state transitions directly with `#[test]`
   and local `#[cfg(test)]` modules. Cover normal cases, invalid inputs, boundary
   values, and failure paths. Add a regression test for each bug fix and confirm
   it fails for the original reason before applying the fix where practical.
2. **Rust integration tests:** test public backend workflows and real adapters
   where useful. The crate currently builds as `cdylib`/`staticlib`; external
   `rust/tests/` imports may require an intentional `rlib` crate-type addition.
   Prefer unit tests when no separate public-API test is needed.
3. **Dart/widget tests:** test presentation, user actions, loading, errors, and
   success states using a fake backend, without a phone or native library.
4. **Real-bridge integration tests:** keep a focused suite for native loading,
   initialization, argument/result/error mapping, streams or lifecycle behavior
   when used, and a few critical end-to-end workflows. Rust tests alone do not
   prove generated bindings work.
5. **Device/emulator checks:** reserve these for platform plugins, permissions,
   native packaging, lifecycle, rendering, and other device-specific behavior.
   Automate them when practical; state exactly what still needs manual checking.

Tests must be isolated and deterministic: no live services, developer credentials,
shared persistent state, wall-clock sleeps, or test-order dependencies in the
headless suite. Use temporary storage and controlled inputs. Prefer observable
behavior over assertions about implementation details. Use property-based tests
for suitable invariants when their value justifies adding the dependency.

## Generated code and bridge changes

- Do not hand-edit `rust/src/frb_generated.rs` or files under `lib/src/rust/`,
  including `api/*.dart`. Change Rust API sources/configuration and regenerate.
- Run `flutter_rust_bridge_codegen generate` from the repository root after
  changing bridge-visible signatures, types, modules, or annotations. Use a
  generator matching the FRB version in both manifests (currently `2.13.0`).
  Backend-only implementation changes do not normally require regeneration.
- Review and include the resulting generated diff with its source change.
  Do not fix generator failures by patching the output.
- Treat `rust_builder/` and platform scaffolding as build infrastructure: modify
  only when the task requires it. Do not casually patch vendored Cargokit.
- Do not upgrade FRB or other dependencies as unrelated cleanup.

## Verification

Run commands from the repository root. Start with focused tests, then run the
relevant checks below before declaring the change complete.

For Rust changes:

```sh
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path rust/Cargo.toml
```

For handwritten Dart/UI changes, or regenerated Dart bindings:

```sh
dart format --output=none --set-exit-if-changed lib test integration_test
flutter analyze
flutter test
```

`flutter test` covers tests under `test/`; add applicable tests there as behavior
is introduced. It is not a substitute for the existing native integration test.
For bridge or native-build changes, also run on an available supported target:

```sh
flutter test integration_test/simple_test.dart -d <device-id>
```

Replace `<device-id>` with a target listed by `flutter devices`; run other
relevant integration tests as they are added. A desktop run can exercise the
bridge without a phone, but does not establish Android/iOS packaging correctness.

Do not weaken lints or edit generated output just to make checks pass. Report
commands actually run, failures, missing toolchains/targets, and untested paths.
Never describe a skipped check as passing. For documentation-only changes,
verify paths and commands against the repository; app builds are unnecessary.

## Scope discipline

Keep changes focused. Do not add speculative abstractions, change platform
configuration, or refactor unrelated code. When a feature cannot be tested
headlessly, explain the constraint and retain the smallest meaningful bridge or
platform check rather than omitting coverage entirely.
