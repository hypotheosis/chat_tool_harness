# Releasing

Pushing a tag `v<major>.<minor>.<patch>` (for example `v0.1.0`; a suffix such
as `v0.2.0-rc1` makes a pre-release) runs `.github/workflows/release.yml`. It:

1. runs `flutter analyze`, `flutter test` and `cargo test`;
2. builds a release APK for arm64, armv7 and x86_64, with the tag as the
   version name and the workflow run number as the version code;
3. signs it with the release key from the repository secrets;
4. publishes a GitHub release with the APK, its SHA-256 and notes listing the
   commits since the previous `v*` tag.

```sh
git tag v0.1.0
git push origin v0.1.0
```

## Signing key

Every release must be signed with the same key, or Android refuses to install
it over an earlier release (the user would have to uninstall and lose their
data). The workflow reads these repository secrets:

| Secret | Value |
| --- | --- |
| `ANDROID_KEYSTORE_BASE64` | the PKCS12 keystore, base64-encoded |
| `ANDROID_KEYSTORE_PASSWORD` | the store password |
| `ANDROID_KEY_ALIAS` | the key alias |
| `ANDROID_KEY_PASSWORD` | the key password |

`android/app/build.gradle.kts` signs release builds with the keystore named by
`ANDROID_KEYSTORE_PATH` (and the password and alias variables above) when it
is set, and with the debug key otherwise. To build a signed release locally:

```sh
export ANDROID_KEYSTORE_PATH=/path/to/release.jks
export ANDROID_KEYSTORE_PASSWORD=... ANDROID_KEY_ALIAS=... ANDROID_KEY_PASSWORD=...
flutter build apk --release
```

Keep a backup of the keystore and its passwords outside the repository. If
the key is lost, later releases cannot update existing installs.

Debug builds (`flutter run`, `flutter build apk --debug`) are signed with the
local debug key, so a release cannot be installed over a debug build (or the
reverse) without uninstalling first.
