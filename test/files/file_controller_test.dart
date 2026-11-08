import 'dart:async';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/files/file_controller.dart';
import 'package:flutter_test/flutter_test.dart';
import '../support/fake_backend.dart';

const summary = ArtifactSummary(
  id: 'a',
  conversationId: 'owner',
  name: 'notes.md',
  bytes: 7,
);
const content = ArtifactContent(
  summary: summary,
  text: '<script>literal</script> ![](https://invalid/image)',
  privatePath: '/private/opaque.txt',
);

class FilesBackend extends FakeBackend {
  @override
  Future<CommandReply> execute(AppCommand command) async {
    await super.execute(command);
    return command is ReadArtifact
        ? const CommandReply(artifact: content)
        : const CommandReply();
  }
}

class Documents implements AndroidDocumentAccess {
  final saved = <ArtifactContent>[];
  final shared = <ArtifactContent>[];
  Completer<DocumentResult>? pending;
  @override
  Future<DocumentResult> saveArtifact(ArtifactContent a) async {
    saved.add(a);
    return pending?.future ?? DocumentResult.cancelled;
  }

  @override
  Future<DocumentResult> shareArtifact(ArtifactContent a) async {
    shared.add(a);
    return DocumentResult.shared;
  }
}

void main() {
  test(
    'preview is literal, list is conversation-owned, neither exports',
    () async {
      final backend = FilesBackend()
        ..current = const AppSnapshot(
          artifacts: [
            summary,
            ArtifactSummary(
              id: 'foreign',
              conversationId: 'other',
              name: 'x.txt',
              bytes: 1,
            ),
          ],
        );
      final documents = Documents();
      final controller = FileController(backend, documents, 'owner');
      expect(await controller.preview('a'), content.text);
      expect((await controller.list()).map((a) => a.id), ['a']);
      expect(documents.saved, isEmpty);
      expect(documents.shared, isEmpty);
      expect((backend.commands.single as ReadArtifact).conversationId, 'owner');
      await backend.close();
    },
  );
  test(
    'explicit Save revalidates ownership, cancellation is not success, no automatic share',
    () async {
      final backend = FilesBackend();
      final documents = Documents();
      final c = FileController(backend, documents, 'owner');
      expect(await c.save('a'), DocumentResult.cancelled);
      expect(documents.saved.single, content);
      expect(documents.shared, isEmpty);
      backend.failure = const BackendFailure(FailureCode.conflict);
      await expectLater(c.share('foreign'), throwsA(isA<BackendFailure>()));
      expect(documents.shared, isEmpty);
      await backend.close();
    },
  );
  test(
    'picker busy rejects another export and releases after cancellation',
    () async {
      final b = FilesBackend();
      final d = Documents()..pending = Completer<DocumentResult>();
      final c = FileController(b, d, 'owner');
      final saving = c.save('a');
      await Future<void>.delayed(Duration.zero);
      await expectLater(c.share('a'), throwsA(isA<BackendFailure>()));
      d.pending!.complete(DocumentResult.cancelled);
      expect(await saving, DocumentResult.cancelled);
      expect(await c.share('a'), DocumentResult.shared);
      await c.delete('a');
      expect(b.commands.last, isA<DeleteArtifact>());
      await b.close();
    },
  );
}
