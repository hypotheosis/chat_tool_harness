#ifndef RUNNER_PLATFORM_CHANNELS_H_
#define RUNNER_PLATFORM_CHANNELS_H_

#include <flutter_linux/flutter_linux.h>

// The Linux side of the app's platform channels (the Android side lives in
// android/app/src/main/kotlin):
//
// - chat_tool_harness/credentials: put/resolve/describe/delete against the
//   Secret Service (GNOME Keyring, KWallet) through libsecret, one operation
//   at a time like Android's single-thread executor.
// - chat_tool_harness/agent_runtime and chat_tool_harness/agent_lifecycle:
//   Linux has no background host, so runs live as long as the window. The
//   runtime accepts start/stop and reports state; lifecycle never emits.
// - chat_tool_harness/documents: no file chooser or share sheet yet; every
//   call fails with UNSUPPORTED_PLATFORM.
//
// The PDF worker channels are not registered on Linux.
typedef struct _PlatformChannels PlatformChannels;

// Registers the channels on `messenger`. They stay registered until
// platform_channels_free.
PlatformChannels* platform_channels_new(FlBinaryMessenger* messenger);

// Unregisters the channels. Credential operations already in flight keep
// their own references and finish on their own.
void platform_channels_free(PlatformChannels* channels);

#endif  // RUNNER_PLATFORM_CHANNELS_H_
