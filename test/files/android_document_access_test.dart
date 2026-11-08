import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/files/android_document_access.dart';
import 'package:chat_tool_harness/features/files/file_controller.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const access = MethodChannelDocumentAccess();
  const content = ArtifactContent(
    summary: ArtifactSummary(
      id: 'id',
      conversationId: 'owner',
      name: 'note.md',
      bytes: 1,
    ),
    text: 'x',
    privatePath: '/opaque.txt',
  );
  final calls = <MethodCall>[];
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  tearDown(
    () => messenger.setMockMethodCallHandler(
      MethodChannelDocumentAccess.channel,
      null,
    ),
  );
  test(
    'native Save cancellation and Share launch retain typed results and private paths',
    () async {
      messenger.setMockMethodCallHandler(MethodChannelDocumentAccess.channel, (
        call,
      ) async {
        calls.add(call);
        return call.method == 'saveArtifact' ? 'cancelled' : 'shared';
      });
      expect(await access.saveArtifact(content), DocumentResult.cancelled);
      expect(await access.shareArtifact(content), DocumentResult.shared);
      expect(calls.map((c) => c.method), ['saveArtifact', 'shareArtifact']);
      expect(calls.first.arguments, {
        'privatePath': '/opaque.txt',
        'name': 'note.md',
      });
    },
  );
  test(
    'native errors are payload-free failures, never successful exports',
    () async {
      messenger.setMockMethodCallHandler(MethodChannelDocumentAccess.channel, (
        _,
      ) async {
        throw PlatformException(code: 'secret', message: 'sensitive URI');
      });
      await expectLater(
        access.saveArtifact(content),
        throwsA(
          isA<BackendFailure>().having(
            (e) => e.code,
            'safe failure',
            FailureCode.unavailableTransport,
          ),
        ),
      );
      messenger.setMockMethodCallHandler(
        MethodChannelDocumentAccess.channel,
        (_) async => 'unexpected',
      );
      await expectLater(
        access.shareArtifact(content),
        throwsA(
          isA<BackendFailure>().having(
            (e) => e.code,
            'bad protocol',
            FailureCode.protocol,
          ),
        ),
      );
    },
  );
}
