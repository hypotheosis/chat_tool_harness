#include "credential_rules.h"

#include <cstring>

namespace {

constexpr uint8_t kSpace = 0x20;

bool IsAlpha(uint8_t b) {
  return (b >= 'A' && b <= 'Z') || (b >= 'a' && b <= 'z');
}

bool IsDigit(uint8_t b) { return b >= '0' && b <= '9'; }

// RFC 9110 tchar, as in CredentialStore.isScheme.
bool IsTchar(uint8_t b) {
  return IsAlpha(b) || IsDigit(b) ||
         (b != 0 && std::strchr("!#$%&'*+.^_`|~-", b) != nullptr);
}

bool IsScheme(const uint8_t* value, size_t end) {
  if (!IsAlpha(value[0])) return false;
  for (size_t i = 0; i < end; i++) {
    if (!IsTchar(value[i])) return false;
  }
  return true;
}

bool IsSendable(const uint8_t* value, size_t length) {
  if (length == 0 || value[0] == kSpace || value[length - 1] == kSpace) {
    return false;
  }
  for (size_t i = 0; i < length; i++) {
    const uint8_t b = value[i];
    if (b == kSpace) {
      // i > 0: the first byte is not a space.
      if (value[i - 1] == kSpace) return false;
    } else if (b < 0x21 || b > 0x7E) {
      return false;
    }
  }
  return true;
}

}  // namespace

bool IsCanonicalReference(const char* value) {
  if (value == nullptr || std::strlen(value) != 36) return false;
  for (size_t i = 0; i < 36; i++) {
    const char c = value[i];
    if (i == 8 || i == 13 || i == 18 || i == 23) {
      if (c != '-') return false;
    } else if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f'))) {
      return false;
    }
  }
  return true;
}

CredentialSummary DescribeCredential(const uint8_t* value, size_t length) {
  CredentialSummary summary;
  summary.length = length;
  summary.sendable = IsSendable(value, length);
  const void* found = length == 0 ? nullptr : std::memchr(value, kSpace, length);
  if (found != nullptr) {
    const size_t space =
        static_cast<size_t>(static_cast<const uint8_t*>(found) - value);
    if (space > 0 && space < length - 1 && IsScheme(value, space)) {
      summary.has_scheme = true;
      summary.scheme.assign(reinterpret_cast<const char*>(value), space);
    }
  }
  return summary;
}
