import 'dart:io';
import 'dart:convert';
import 'package:flutter/services.dart';
import 'backend.dart';
import 'frb_backend.dart';
import 'models.dart';
import 'engine_lifecycle.dart';

/// What the device reports about a stored credential; never the secret.
class CredentialSummary {
  const CredentialSummary({this.scheme, required this.length});

  /// Auth scheme the stored value starts with, or null when `Bearer` is added.
  final String? scheme;

  /// Stored value length in UTF-8 bytes.
  final int length;

  /// What will be sent for an Authorization header.
  String get authorizationPreview => scheme != null
      ? '$scheme ••• ($length chars)'
      : 'Bearer ••• (${length + 7} chars, Bearer added)';
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

  /// Null when no credential is stored under [reference]; decryption and
  /// keystore failures throw.
  static Future<CredentialSummary?> describeCredential(String reference) async {
    final reply = await credentials.invokeMapMethod<String, Object?>(
      'describe',
      {'reference': reference},
    );
    if (reply == null) return null;
    final length = reply['length'];
    final scheme = reply['scheme'];
    if (length is! int || (scheme != null && scheme is! String)) {
      throw const BackendFailure(FailureCode.authentication);
    }
    return CredentialSummary(scheme: scheme as String?, length: length);
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
