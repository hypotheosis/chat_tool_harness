#ifndef RUNNER_CREDENTIAL_RULES_H_
#define RUNNER_CREDENTIAL_RULES_H_

#include <cstddef>
#include <cstdint>
#include <string>

// Pure rules the Linux credential channel shares with Android's
// CredentialStore.kt. They take bytes, never a std::string copy of a secret.

// Values the store accepts, in UTF-8 bytes (CredentialStore.put).
constexpr size_t kMaxCredentialBytes = 8192;

// True when `value` is a lowercase canonical UUID, the only reference form
// Android accepts (UUID.fromString(x).toString() == x). Null is false.
bool IsCanonicalReference(const char* value);

// What `describe` reports about a stored value; never the value itself.
struct CredentialSummary {
  // The auth scheme the value starts with ("Bearer" in "Bearer x"), or
  // empty when there is none and Rust will add "Bearer ".
  std::string scheme;
  bool has_scheme = false;
  // UTF-8 byte length of the stored value.
  size_t length = 0;
  // Every byte is visible ASCII or a single space between visible bytes.
  bool sendable = false;
};

// The same rules as CredentialStore.describe on Android, so the cases in
// test/fixtures/authorization_parity.json give the same answers.
CredentialSummary DescribeCredential(const uint8_t* value, size_t length);

#endif  // RUNNER_CREDENTIAL_RULES_H_
