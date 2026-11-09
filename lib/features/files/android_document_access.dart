import 'package:flutter/services.dart';
import '../../backend/models.dart';
import 'file_controller.dart';

class MethodChannelDocumentAccess implements AndroidDocumentAccess {
  const MethodChannelDocumentAccess();
  static const channel = MethodChannel('chat_tool_harness/documents');

  /// The error code a platform without document access replies with.
  static const unsupportedPlatformCode = 'UNSUPPORTED_PLATFORM';
  Future<DocumentResult> _invoke(
    String method,
    ArtifactContent artifact,
  ) async {
    try {
      final result = await channel.invokeMethod<String>(method, {
        'privatePath': artifact.privatePath,
        'name': artifact.summary.name,
      });
      return switch (result) {
        'saved' => DocumentResult.saved,
        'shared' => DocumentResult.shared,
        'cancelled' => DocumentResult.cancelled,
        _ => throw const BackendFailure(FailureCode.protocol),
      };
    } on PlatformException catch (e) {
      // The Linux runner answers this way: it has no file chooser or share
      // sheet yet.
      if (e.code == unsupportedPlatformCode) {
        throw const BackendFailure(FailureCode.unsupportedPlatform);
      }
      throw const BackendFailure(FailureCode.unavailableTransport);
    } on MissingPluginException {
      throw const BackendFailure(FailureCode.unavailableTransport);
    }
  }

  @override
  Future<DocumentResult> saveArtifact(ArtifactContent artifact) =>
      _invoke('saveArtifact', artifact);
  @override
  Future<DocumentResult> shareArtifact(ArtifactContent artifact) =>
      _invoke('shareArtifact', artifact);
}
