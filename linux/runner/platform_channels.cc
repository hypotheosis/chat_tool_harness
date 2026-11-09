#include "platform_channels.h"

#include <libsecret/secret.h>

#include <cstring>
#include <deque>
#include <memory>
#include <string>

#include "credential_rules.h"

namespace {

constexpr char kCredentialError[] = "CREDENTIAL_UNAVAILABLE";
constexpr char kCredentialMessage[] =
    "Credential unavailable. Delete and re-enter it.";
constexpr char kUnsupported[] = "UNSUPPORTED_PLATFORM";

// Items carry this schema and a "reference" attribute holding the credential
// reference (a UUID); the secret is the stored value's bytes.
const SecretSchema* CredentialSchema() {
  static const SecretSchema schema = {
      "com.example.chat_tool_harness.Credential",
      SECRET_SCHEMA_NONE,
      {
          {"reference", SECRET_SCHEMA_ATTRIBUTE_STRING},
          {"NULL", SECRET_SCHEMA_ATTRIBUTE_STRING},
      }};
  return &schema;
}

void Respond(FlMethodCall* call, FlValue* result) {
  g_autoptr(GError) error = nullptr;
  if (!fl_method_call_respond_success(call, result, &error)) {
    g_warning("Failed to send a platform channel reply: %s", error->message);
  }
}

void RespondError(FlMethodCall* call, const char* code, const char* message) {
  g_autoptr(GError) error = nullptr;
  if (!fl_method_call_respond_error(call, code, message, nullptr, &error)) {
    g_warning("Failed to send a platform channel error: %s", error->message);
  }
}

// The string argument `key`, or null when it is absent or not a string.
const char* StringArgument(FlMethodCall* call, const char* key) {
  FlValue* args = fl_method_call_get_args(call);
  if (args == nullptr || fl_value_get_type(args) != FL_VALUE_TYPE_MAP) {
    return nullptr;
  }
  FlValue* value = fl_value_lookup_string(args, key);
  if (value == nullptr || fl_value_get_type(value) != FL_VALUE_TYPE_STRING) {
    return nullptr;
  }
  return fl_value_get_string(value);
}

// ---------------------------------------------------------------------------
// Credentials

enum class CredentialOp { kPut, kResolve, kDescribe, kDelete };

struct CredentialStore;

// One queued call. It holds the call and the store alive until it is answered.
struct PendingCredential {
  std::shared_ptr<CredentialStore> store;
  FlMethodCall* call;
  CredentialOp op;
  std::string reference;
};

// Runs one Secret Service operation at a time, in arrival order, as Android's
// single-thread executor does, so a put is stored before a later resolve.
struct CredentialStore : std::enable_shared_from_this<CredentialStore> {
  std::deque<PendingCredential*> queue;
  bool busy = false;

  void Enqueue(FlMethodCall* call, CredentialOp op, const char* reference) {
    queue.push_back(new PendingCredential{
        shared_from_this(), FL_METHOD_CALL(g_object_ref(call)), op, reference});
    if (!busy) StartNext();
  }

  void StartNext();
  void Finish(PendingCredential* pending);
};

void Finish(PendingCredential* pending) { pending->store->Finish(pending); }

void OnStored(GObject*, GAsyncResult* result, gpointer data) {
  auto* pending = static_cast<PendingCredential*>(data);
  g_autoptr(GError) error = nullptr;
  if (secret_password_store_finish(result, &error)) {
    Respond(pending->call, nullptr);
  } else {
    RespondError(pending->call, kCredentialError, kCredentialMessage);
  }
  Finish(pending);
}

FlValue* SummaryValue(const CredentialSummary& summary) {
  FlValue* map = fl_value_new_map();
  fl_value_set_string_take(map, "scheme",
                           summary.has_scheme
                               ? fl_value_new_string(summary.scheme.c_str())
                               : fl_value_new_null());
  fl_value_set_string_take(map, "length",
                           fl_value_new_int(static_cast<int64_t>(summary.length)));
  fl_value_set_string_take(map, "sendable", fl_value_new_bool(summary.sendable));
  return map;
}

void OnLookedUp(GObject*, GAsyncResult* result, gpointer data) {
  auto* pending = static_cast<PendingCredential*>(data);
  g_autoptr(GError) error = nullptr;
  SecretValue* secret = secret_password_lookup_binary_finish(result, &error);
  if (error != nullptr) {
    RespondError(pending->call, kCredentialError, kCredentialMessage);
  } else if (secret == nullptr) {
    // Nothing is stored: Rust reports CredentialMissing.
    Respond(pending->call, nullptr);
  } else {
    gsize length = 0;
    const auto* bytes =
        reinterpret_cast<const uint8_t*>(secret_value_get(secret, &length));
    if (pending->op == CredentialOp::kDescribe) {
      g_autoptr(FlValue) summary =
          SummaryValue(DescribeCredential(bytes, length));
      Respond(pending->call, summary);
    } else {
      g_autoptr(FlValue) reply = fl_value_new_uint8_list(bytes, length);
      Respond(pending->call, reply);
      // The codec has encoded the reply; clear this copy.
      std::memset(const_cast<uint8_t*>(fl_value_get_uint8_list(reply)), 0,
                  length);
    }
    // SecretValue keeps the secret in non-pageable memory and clears it.
    secret_value_unref(secret);
  }
  Finish(pending);
}

void OnCleared(GObject*, GAsyncResult* result, gpointer data) {
  auto* pending = static_cast<PendingCredential*>(data);
  g_autoptr(GError) error = nullptr;
  // False without an error means nothing was stored, which is a success.
  secret_password_clear_finish(result, &error);
  if (error == nullptr) {
    Respond(pending->call, nullptr);
  } else {
    RespondError(pending->call, kCredentialError, kCredentialMessage);
  }
  Finish(pending);
}

void CredentialStore::StartNext() {
  if (queue.empty()) {
    busy = false;
    return;
  }
  busy = true;
  PendingCredential* pending = queue.front();
  queue.pop_front();
  const char* reference = pending->reference.c_str();
  switch (pending->op) {
    case CredentialOp::kPut: {
      FlValue* args = fl_method_call_get_args(pending->call);
      FlValue* value = fl_value_lookup_string(args, "value");
      const uint8_t* bytes = fl_value_get_uint8_list(value);
      const size_t length = fl_value_get_length(value);
      SecretValue* secret =
          secret_value_new(reinterpret_cast<const gchar*>(bytes),
                           static_cast<gssize>(length),
                           "application/octet-stream");
      // libsecret has its own copy; clear the one the codec decoded.
      std::memset(const_cast<uint8_t*>(bytes), 0, length);
      g_autofree gchar* label =
          g_strdup_printf("chat_tool_harness credential %s", reference);
      secret_password_store_binary(CredentialSchema(), SECRET_COLLECTION_DEFAULT,
                                   label, secret, nullptr, OnStored, pending,
                                   "reference", reference, nullptr);
      secret_value_unref(secret);
      break;
    }
    case CredentialOp::kResolve:
    case CredentialOp::kDescribe:
      secret_password_lookup(CredentialSchema(), nullptr, OnLookedUp, pending,
                             "reference", reference, nullptr);
      break;
    case CredentialOp::kDelete:
      secret_password_clear(CredentialSchema(), nullptr, OnCleared, pending,
                            "reference", reference, nullptr);
      break;
  }
}

void CredentialStore::Finish(PendingCredential* pending) {
  g_object_unref(pending->call);
  // `pending` may hold the last reference to this store.
  std::shared_ptr<CredentialStore> self = std::move(pending->store);
  delete pending;
  self->StartNext();
}

// Validates the arguments here, before queueing, and answers bad ones with
// the same error Android gives.
void HandleCredentialCall(const std::shared_ptr<CredentialStore>& store,
                          FlMethodCall* call) {
  const char* method = fl_method_call_get_name(call);
  CredentialOp op;
  if (strcmp(method, "put") == 0) {
    op = CredentialOp::kPut;
  } else if (strcmp(method, "resolve") == 0) {
    op = CredentialOp::kResolve;
  } else if (strcmp(method, "describe") == 0) {
    op = CredentialOp::kDescribe;
  } else if (strcmp(method, "delete") == 0) {
    op = CredentialOp::kDelete;
  } else {
    fl_method_call_respond_not_implemented(call, nullptr);
    return;
  }
  const char* reference = StringArgument(call, "reference");
  if (!IsCanonicalReference(reference)) {
    RespondError(call, kCredentialError, kCredentialMessage);
    return;
  }
  if (op == CredentialOp::kPut) {
    FlValue* value =
        fl_value_lookup_string(fl_method_call_get_args(call), "value");
    if (value == nullptr ||
        fl_value_get_type(value) != FL_VALUE_TYPE_UINT8_LIST ||
        fl_value_get_length(value) == 0 ||
        fl_value_get_length(value) > kMaxCredentialBytes) {
      RespondError(call, kCredentialError, kCredentialMessage);
      return;
    }
  }
  store->Enqueue(call, op, reference);
}

// ---------------------------------------------------------------------------
// Agent runtime

FlValue* RuntimeState(const char* runtime_id) {
  FlValue* map = fl_value_new_map();
  fl_value_set_string_take(map, "runtimeId", fl_value_new_string(runtime_id));
  fl_value_set_string_take(map, "activeRuns", fl_value_new_int(0));
  fl_value_set_string_take(map, "status", fl_value_new_string("idle"));
  fl_value_set_string_take(map, "runId", fl_value_new_string(""));
  fl_value_set_string_take(map, "lease", fl_value_new_string(""));
  // Linux shows no run notification, so nothing is hidden from the user.
  fl_value_set_string_take(map, "notificationVisibility",
                           fl_value_new_string("normal"));
  return map;
}

void HandleRuntimeCall(const char* runtime_id, FlMethodCall* call) {
  const char* method = fl_method_call_get_name(call);
  if (strcmp(method, "state") == 0) {
    g_autoptr(FlValue) state = RuntimeState(runtime_id);
    Respond(call, state);
  } else if (strcmp(method, "startRunService") == 0 ||
             strcmp(method, "stopRunService") == 0) {
    // There is no host to start or stop; the run lives with the window.
    if (!IsCanonicalReference(StringArgument(call, "runId")) ||
        !IsCanonicalReference(StringArgument(call, "conversationId")) ||
        !IsCanonicalReference(StringArgument(call, "generation"))) {
      RespondError(call, "INVALID_ID", "Invalid identity");
    } else {
      Respond(call, nullptr);
    }
  } else {
    fl_method_call_respond_not_implemented(call, nullptr);
  }
}

FlMethodErrorResponse* OnLifecycleListen(FlEventChannel*, FlValue*, gpointer) {
  return nullptr;
}

FlMethodErrorResponse* OnLifecycleCancel(FlEventChannel*, FlValue*, gpointer) {
  return nullptr;
}

}  // namespace

struct _PlatformChannels {
  std::shared_ptr<CredentialStore> store = std::make_shared<CredentialStore>();
  gchar* runtime_id = g_uuid_string_random();
  FlMethodChannel* credentials = nullptr;
  FlMethodChannel* runtime = nullptr;
  FlEventChannel* lifecycle = nullptr;
  FlMethodChannel* documents = nullptr;
};

PlatformChannels* platform_channels_new(FlBinaryMessenger* messenger) {
  auto* self = new PlatformChannels();
  g_autoptr(FlStandardMethodCodec) codec = fl_standard_method_codec_new();
  FlMethodCodec* method_codec = FL_METHOD_CODEC(codec);

  self->credentials = fl_method_channel_new(
      messenger, "chat_tool_harness/credentials", method_codec);
  fl_method_channel_set_method_call_handler(
      self->credentials,
      [](FlMethodChannel*, FlMethodCall* call, gpointer data) {
        HandleCredentialCall(static_cast<PlatformChannels*>(data)->store, call);
      },
      self, nullptr);

  self->runtime = fl_method_channel_new(
      messenger, "chat_tool_harness/agent_runtime", method_codec);
  fl_method_channel_set_method_call_handler(
      self->runtime,
      [](FlMethodChannel*, FlMethodCall* call, gpointer data) {
        HandleRuntimeCall(static_cast<PlatformChannels*>(data)->runtime_id,
                          call);
      },
      self, nullptr);

  self->lifecycle = fl_event_channel_new(
      messenger, "chat_tool_harness/agent_lifecycle", method_codec);
  fl_event_channel_set_stream_handlers(self->lifecycle, OnLifecycleListen,
                                       OnLifecycleCancel, nullptr, nullptr);

  self->documents = fl_method_channel_new(
      messenger, "chat_tool_harness/documents", method_codec);
  fl_method_channel_set_method_call_handler(
      self->documents,
      [](FlMethodChannel*, FlMethodCall* call, gpointer) {
        RespondError(call, kUnsupported,
                     "Saving and sharing files is not available on Linux.");
      },
      nullptr, nullptr);
  return self;
}

void platform_channels_free(PlatformChannels* self) {
  if (self == nullptr) return;
  g_clear_object(&self->credentials);
  g_clear_object(&self->runtime);
  g_clear_object(&self->lifecycle);
  g_clear_object(&self->documents);
  g_free(self->runtime_id);
  delete self;
}
