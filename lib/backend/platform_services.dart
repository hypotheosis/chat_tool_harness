import 'dart:io';
import 'dart:convert';
import 'package:flutter/services.dart';
import 'backend.dart';
import 'frb_backend.dart';
import 'models.dart';
import 'engine_lifecycle.dart';

/// What the device reports about a stored credential; never the secret.
class CredentialSummary {
  const CredentialSummary({
    this.scheme,
    required this.length,
    required this.sendable,
  });

  /// Auth scheme the stored value starts with, or null when `Bearer` is added.
  final String? scheme;

  /// Stored value length in UTF-8 bytes.
  final int length;

  /// Whether every byte is visible ASCII or a single space between visible
  /// characters: no tabs, newlines, non-ASCII, or leading, trailing or
  /// consecutive spaces. False usually means a paste picked up whitespace;
  /// servers may then reject the key.
  final bool sendable;

  /// The Authorization header that will be sent; the count is the secret's
  /// length, excluding any stored scheme and its space.
  String get authorizationPreview {
    final scheme = this.scheme;
    final header = scheme != null
        ? 'Authorization: $scheme ••• (${length - scheme.length - 1} chars)'
        : 'Authorization: Bearer ••• ($length chars; Bearer added)';
    return sendable
        ? header
        : '$header. Has leading/trailing whitespace or characters that '
              'cannot be sent';
  }
}

class PlatformServices {
  const PlatformServices({
    required this.resolveCredential,
    required this.setServiceActive,
  });
  final Future<String?> Function(String reference) resolveCredential;
  final Future<void> Function(RunServiceRequest request) setServiceActive;

  static const credentials = MethodChannel('chat_tool_harness/credentials');
  static Future<void> putCredential(String reference, String value) async {
    final bytes = Uint8List.fromList(utf8.encode(value));
    try {
      await credentials.invokeMethod<void>('put', {
        'reference': reference,
        'value': bytes,
      });
    } finally {
      bytes.fillRange(0, bytes.length, 0);
    }
  }

  static Future<void> deleteCredential(String reference) =>
      credentials.invokeMethod<void>('delete', {'reference': reference});

  /// Describes the credential stored under [reference] without reading it
  /// into Dart; null when nothing is stored there.
  ///
  /// Throws [BackendFailure] with [FailureCode.authentication] when the
  /// device cannot read the credential (corrupt or unreadable file, lost
  /// keystore key; the user must re-enter it), and with
  /// [FailureCode.protocol] when the platform reply is malformed.
  static Future<CredentialSummary?> describeCredential(String reference) async {
    final Object? reply;
    try {
      reply = await credentials.invokeMethod<Object?>('describe', {
        'reference': reference,
      });
    } on PlatformException {
      throw const BackendFailure(FailureCode.authentication);
    }
    if (reply == null) return null;
    if (reply is! Map) throw const BackendFailure(FailureCode.protocol);
    final length = reply['length'];
    final scheme = reply['scheme'];
    final sendable = reply['sendable'];
    if (length is! int ||
        sendable is! bool ||
        (scheme != null && scheme is! String)) {
      throw const BackendFailure(FailureCode.protocol);
    }
    return CredentialSummary(
      scheme: scheme as String?,
      length: length,
      sendable: sendable,
    );
  }

  factory PlatformServices.android({
    Future<String?> Function(String)? resolveCredential,
  }) => PlatformServices(
    resolveCredential:
        resolveCredential ??
        (reference) async {
          final bytes = await credentials.invokeMethod<Uint8List>('resolve', {
            'reference': reference,
          });
          // Null means no credential is stored: Rust reports CredentialMissing.
          if (bytes == null) return null;
          // Codec results are read-only views; Strings/transport buffers are transient, not wipeable.
          return utf8.decode(bytes);
        },
    setServiceActive: (request) => runtime.invokeMethod<void>(
      request.active ? 'startRunService' : 'stopRunService',
      {
        'runId': request.runId,
        'generation': request.generation,
        'conversationId': request.conversationId,
        'waitingApproval': request.waitingApproval,
      },
    ),
  );
  static const runtime = MethodChannel('chat_tool_harness/agent_runtime');

  static Future<String> applicationDataDir() async {
    if (!Platform.isAndroid) {
      throw const BackendFailure(FailureCode.configuration);
    }
    // Flutter's Android VM temp directory is the application's private cache.
    // Rust owns directory creation and maps filesystem failures to CoreError.
    try {
      return '${Directory.systemTemp.parent.path}/files/agent';
    } catch (_) {
      throw const BackendFailure(FailureCode.storage);
    }
  }
}

/// One owner per Dart engine/isolate; widgets borrow it, never close it.
class EngineBackend {
  EngineBackend._();
  static final owner = BackendOwner(
    () async => FrbBackend.open(
      await PlatformServices.applicationDataDir(),
      PlatformServices.android(),
    ),
  );
  static final lifecycle = EngineLifecycle(() => owner.backend);
  static Future<void> initialize() async {
    lifecycle.initialize();
    await owner.backend;
    final state = await PlatformServices.runtime
        .invokeMapMethod<String, dynamic>('state');
    if (state?['notificationVisibility'] == 'reduced') {
      lifecycle.notice.value =
          'Notifications are disabled: reduced background visibility, not a foreground-service prohibition. Enable notifications in Android settings.';
    }
  }

  static Future<void> close() async {
    await lifecycle.close();
    await owner.close();
  }
}
