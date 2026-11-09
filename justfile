# The verification commands from AGENTS.md. Run from the repository root.

rust := "--manifest-path rust/Cargo.toml"
dart_dirs := "lib test integration_test"

# List the recipes.
default:
    @just --list

# Format Rust and Dart sources in place.
fmt:
    cargo fmt {{rust}}
    dart format {{dart_dirs}}

# Fail if anything is not formatted.
fmt-check:
    cargo fmt {{rust}} -- --check
    dart format --output=none --set-exit-if-changed {{dart_dirs}}

# Lint Rust (warnings are errors) and Dart.
lint:
    cargo clippy {{rust}} --all-targets -- -D warnings
    flutter analyze

# The headless suites: Rust tests and Flutter tests under test/.
test:
    cargo test {{rust}}
    flutter test

# Every integration suite on one device, such as `just integration linux`.
integration device:
    for suite in integration_test/*_test.dart; do \
        flutter test "$suite" -d {{device}} || exit 1; \
    done

# Everything AGENTS.md asks for before a change is complete, minus devices.
check: fmt-check lint test
