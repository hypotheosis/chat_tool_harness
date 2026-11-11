import 'dart:async';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/history/history_screen.dart';
import 'package:chat_tool_harness/features/history/history_controller.dart';
import 'package:chat_tool_harness/features/files/file_controller.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import '../support/fake_backend.dart';
import '../support/ui_fixture.dart' as ui;

const row = ConversationSummary(
  id: 'owner',
  title: 'Saved title',
  hasUnknownOutcomes: true,
);
const artifact = ArtifactSummary(
  id: 'a',
  conversationId: 'owner',
  name: 'transcript.md',
  bytes: 12,
);

class HistoryBackend extends FakeBackend {
  final pages = <List<ConversationSummary>>[];
  Completer<CommandReply>? stop;
  Completer<void>? exportGate;
  Completer<void>? searchGate;
  @override
  Future<CommandReply> execute(AppCommand c) async {
    await super.execute(c);
    if (c is SearchConversations) {
      final gate = searchGate;
      if (gate != null) {
        searchGate = null;
        await gate.future;
      }
      return CommandReply(history: pages.isEmpty ? [row] : pages.removeAt(0));
    }
    if (c is ExportTranscript) {
      await exportGate?.future;
      return const CommandReply(
        exportBundle: ExportBundle(transcript: artifact),
      );
    }
    if (c is ReadArtifact) {
      return const CommandReply(
        artifact: ArtifactContent(
          summary: artifact,
          text: 'redacted literal <script>',
          privatePath: '/private/a.txt',
        ),
      );
    }
    if (c is CancelRun && stop != null) return stop!.future;
    return const CommandReply();
  }
}

class Documents implements AndroidDocumentAccess {
  int saves = 0, shares = 0;
  @override
  Future<DocumentResult> saveArtifact(ArtifactContent a) async {
    saves++;
    return DocumentResult.cancelled;
  }

  @override
  Future<DocumentResult> shareArtifact(ArtifactContent a) async {
    shares++;
    return DocumentResult.shared;
  }
}

void main() {
  testWidgets('active Stop waits before enabling explicit unknown deletion', (
    tester,
  ) async {
    const activeRow = ConversationSummary(
      id: 'c',
      title: 'Active',
      hasUnknownOutcomes: true,
    );
    final b = HistoryBackend()
      ..current = AppSnapshot(runs: [ui.run(runState: RunState.running)])
      ..stop = Completer<CommandReply>()
      ..pages.addAll([
        [activeRow],
        [activeRow],
      ]);
    await tester.pumpWidget(MaterialApp(home: HistoryScreen(backend: b)));
    await tester.pumpAndSettle();
    expect(
      tester
          .widget<IconButton>(find.widgetWithIcon(IconButton, Icons.delete))
          .onPressed,
      isNull,
    );
    await tester.tap(find.text('Stop and wait'));
    await tester.pump();
    expect(b.commands.whereType<CancelRun>().single.runId, 'r');
    expect(b.commands.whereType<DeleteConversation>(), isEmpty);
    expect(
      tester
          .widget<IconButton>(find.widgetWithIcon(IconButton, Icons.delete))
          .onPressed,
      isNull,
    );
    b.current = AppSnapshot(
      runs: [
        ui.run(
          runState: RunState.cancelled,
          state: AttemptState.outcomeUnknown,
        ),
      ],
    );
    b.stop!.complete(const CommandReply());
    await tester.pumpAndSettle();
    expect(
      tester
          .widget<IconButton>(find.widgetWithIcon(IconButton, Icons.delete))
          .onPressed,
      isNotNull,
    );
    expect(b.commands.whereType<DeleteConversation>(), isEmpty);
    await tester.pumpWidget(const SizedBox());
    await b.close();
  });
  testWidgets(
    'history search rename and explicit unknown delete confirmation',
    (tester) async {
      final b = HistoryBackend();
      await tester.pumpWidget(MaterialApp(home: HistoryScreen(backend: b)));
      await tester.pumpAndSettle();
      expect(find.text('Saved title'), findsOneWidget);
      await tester.enterText(find.byKey(const Key('history-search')), "東京 % '");
      await tester.tap(find.text('Search'));
      await tester.pumpAndSettle();
      expect(
        (b.commands.whereType<SearchConversations>().last).query,
        "東京 % '",
      );
      await tester.tap(find.byTooltip('Rename'));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const Key('history-title')),
        'New title',
      );
      await tester.tap(find.text('Rename').last);
      await tester.pumpAndSettle();
      expect(
        b.commands.whereType<RenameConversation>().single.title,
        'New title',
      );
      await tester.tap(find.byTooltip('Delete'));
      await tester.pumpAndSettle();
      expect(find.textContaining('Unknown tool outcomes'), findsWidgets);
      expect(b.commands.whereType<DeleteConversation>(), isEmpty);
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(b.commands.whereType<DeleteConversation>(), isEmpty);
      await tester.tap(find.byTooltip('Delete'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Confirm delete'));
      await tester.pumpAndSettle();
      expect(
        b.commands.whereType<DeleteConversation>().single.confirmedUnknown,
        isTrue,
      );
      await tester.pumpWidget(const SizedBox());
      expect(b.closed, isFalse);
      await b.close();
    },
  );
  testWidgets(
    'transcript preview default summaries optin and explicit save share',
    (tester) async {
      final b = HistoryBackend();
      final documents = Documents();
      await tester.pumpWidget(
        MaterialApp(
          home: HistoryScreen(backend: b, documents: documents),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Export transcript'));
      await tester.pumpAndSettle();
      expect(find.textContaining('not a restorable backup'), findsOneWidget);
      expect(b.commands.whereType<ExportTranscript>(), isEmpty);
      await tester.tap(find.text('Preview'));
      await tester.pumpAndSettle();
      expect(
        b.commands.whereType<ExportTranscript>().last.includeToolPayloads,
        isFalse,
      );
      expect(find.text('redacted literal <script>'), findsOneWidget);
      expect(documents.saves, 0);
      expect(documents.shares, 0);
      await tester.tap(find.text('Save'));
      await tester.pumpAndSettle();
      expect(documents.saves, 1);
      expect(find.text('Cancelled'), findsOneWidget);
      await tester.tap(find.text('Share'));
      await tester.pumpAndSettle();
      expect(documents.shares, 1);
      await tester.tap(find.byType(CheckboxListTile));
      await tester.pumpAndSettle();
      expect(find.text('redacted literal <script>'), findsNothing);
      await tester.tap(find.text('Preview'));
      await tester.pumpAndSettle();
      expect(
        b.commands.whereType<ExportTranscript>().last.includeToolPayloads,
        isTrue,
      );
      await tester.pumpWidget(const SizedBox());
      await b.close();
    },
  );
  test(
    'history controller paginates and missing keys do not hide browsing',
    () async {
      final b = HistoryBackend()
        ..pages.addAll([
          List.filled(100, row),
          [row],
        ]);
      final c = HistoryController(b);
      await c.connect();
      await c.nextPage();
      expect(b.commands.whereType<SearchConversations>().last.offset, 100);
      b.failure = const BackendFailure(FailureCode.authentication);
      await c.preview('owner', false);
      expect(c.error, contains('credential'));
      expect(c.rows, isNotEmpty);
      final blocked = c.error;
      b.failure = const BackendFailure(FailureCode.credentialMissing);
      await c.preview('owner', false);
      expect(c.error, startsWith('Export blocked'));
      expect(c.error, blocked);
      expect(c.rows, isNotEmpty);
      c.dispose();
      expect(b.closed, isFalse);
      await b.close();
    },
  );
  test(
    'change events during an export refresh history once it finishes',
    () async {
      const stale = ConversationSummary(
        id: 'owner',
        title: 'Saved title',
        hasUnknownOutcomes: false,
      );
      final b = HistoryBackend()
        ..pages.addAll([
          [stale],
          [row],
        ])
        ..exportGate = Completer<void>();
      final c = HistoryController(b);
      await c.connect();
      expect(c.rows.single.hasUnknownOutcomes, isFalse);
      final export = c.preview('owner', false);
      await pumpEventQueue();
      expect(c.loading, isTrue);
      b.emit(const AppEvent(sequence: 1, conversationId: 'owner'));
      b.emit(const AppEvent(sequence: 2, conversationId: 'owner'));
      await pumpEventQueue();
      expect(b.commands.whereType<SearchConversations>(), hasLength(1));
      b.exportGate!.complete();
      await export;
      await pumpEventQueue();
      expect(b.commands.whereType<SearchConversations>(), hasLength(2));
      expect(c.rows.single.hasUnknownOutcomes, isTrue);
      expect(c.loading, isFalse);
      c.dispose();
      await b.close();
    },
  );
  test('a change during a search runs one more search afterwards', () async {
    final b = HistoryBackend();
    final c = HistoryController(b);
    await c.connect();
    final gate = b.searchGate = Completer<void>();
    final search = c.search('q');
    await pumpEventQueue();
    b.emit(const AppEvent(sequence: 1, conversationId: 'owner'));
    await pumpEventQueue();
    expect(b.commands.whereType<SearchConversations>(), hasLength(2));
    gate.complete();
    await search;
    await pumpEventQueue();
    final searches = b.commands.whereType<SearchConversations>().toList();
    expect(searches, hasLength(3));
    expect(searches.last.query, 'q');
    expect(c.loading, isFalse);
    c.dispose();
    await b.close();
  });
  test('a mutation does not queue a redundant refresh', () async {
    final b = HistoryBackend();
    final c = HistoryController(b);
    await c.connect();
    await c.rename('owner', 'New');
    await pumpEventQueue();
    expect(b.commands.whereType<SearchConversations>(), hasLength(2));
    c.dispose();
    await b.close();
  });
  test('the refresh after a failed action keeps its error', () async {
    final b = HistoryBackend()..exportGate = Completer<void>();
    final c = HistoryController(b);
    await c.connect();
    final export = c.preview('owner', false);
    await pumpEventQueue();
    b.emit(const AppEvent(sequence: 1, conversationId: 'owner'));
    await pumpEventQueue();
    b.exportGate!.completeError(
      const BackendFailure(FailureCode.authentication),
    );
    await export;
    await pumpEventQueue();
    expect(b.commands.whereType<SearchConversations>(), hasLength(2));
    expect(c.error, startsWith('Export blocked'));
    c.dispose();
    await b.close();
  });
}
