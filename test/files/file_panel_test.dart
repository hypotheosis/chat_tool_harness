import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/files/file_controller.dart';
import 'package:chat_tool_harness/features/files/file_panel.dart';
import '../support/ui_fixture.dart';

class Documents implements AndroidDocumentAccess {
  int exports = 0;
  @override
  Future<DocumentResult> saveArtifact(ArtifactContent a) async {
    exports++;
    return DocumentResult.cancelled;
  }

  @override
  Future<DocumentResult> shareArtifact(ArtifactContent a) async {
    exports++;
    return DocumentResult.shared;
  }
}

void main() {
  testWidgets('generated_text_artifact_preview_is_literal_and_export_explicit', (
    t,
  ) async {
    final b = UiBackend();
    final docs = Documents();
    const a = ArtifactSummary(
      id: 'a',
      conversationId: 'c',
      name: 'output.html',
      bytes: 80,
    );
    const text =
        '<script>secret-free</script> ![image](https://example.test/x) https://example.test';
    b.current = const AppSnapshot(artifacts: [a]);
    b.reply = const CommandReply(
      artifact: ArtifactContent(
        summary: a,
        text: text,
        privatePath: '/private',
      ),
    );
    await t.pumpWidget(
      MaterialApp(home: FilePanel(controller: FileController(b, docs, 'c'))),
    );
    await t.pumpAndSettle();
    await t.tap(find.text('output.html'));
    await t.pumpAndSettle();
    expect(find.text(text), findsOneWidget);
    expect(find.byType(Image), findsNothing);
    expect(docs.exports, 0);
    await t.tap(find.text('Share'));
    await t.pumpAndSettle();
    expect(docs.exports, 1);
  });
}
