import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/chat/chat_screen.dart';
import '../support/ui_fixture.dart';

class PendingSendBackend extends UiBackend {
  final pending = Completer<CommandReply>();
  @override
  Future<CommandReply> execute(AppCommand command) async {
    if (command is StartTurn) {
      commands.add(command);
      return pending.future;
    }
    return super.execute(command);
  }
}

void main() {
  testWidgets('interrupted_run_requires_explicit_resume', (t) async {
    final b = UiBackend();
    b.current = AppSnapshot(
      conversations: [conversation],
      runs: [run(runState: RunState.interrupted, state: AttemptState.denied)],
    );
    await t.pumpWidget(
      MaterialApp(
        home: ChatScreen(backend: b, conversationId: 'c'),
      ),
    );
    await t.pumpAndSettle();
    expect(b.commands.whereType<ResumeRun>(), isEmpty);
    await t.scrollUntilVisible(
      find.text('Resume'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.tap(find.text('Resume'));
    await t.pumpAndSettle();
    expect(b.commands.whereType<ResumeRun>().single.runId, 'r');
  });
  testWidgets('markdown_formats_text_without_active_content', (t) async {
    await t.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: SafeMarkdown(
            '# Heading\n**strong** and `code`\n<script>x</script> ![x](https://example.test/x) https://example.test',
          ),
        ),
      ),
    );
    expect(find.text('Heading'), findsOneWidget);
    final rich = t
        .widgetList<SelectableText>(find.byType(SelectableText))
        .where((w) => w.textSpan != null);
    expect(
      rich.any(
        (w) => w.textSpan!.children!.whereType<TextSpan>().any(
          (s) => s.text == 'strong' && s.style?.fontWeight == FontWeight.bold,
        ),
      ),
      isTrue,
    );
    expect(find.byType(Image), findsNothing);
    expect(find.textContaining('<script>'), findsOneWidget);
  });
  testWidgets('dispose_during_send_never_cancels_or_uses_disposed_widgets', (
    t,
  ) async {
    final b = PendingSendBackend();
    await t.pumpWidget(
      MaterialApp(
        home: ChatScreen(backend: b, conversationId: 'c'),
      ),
    );
    await t.pumpAndSettle();
    await t.scrollUntilVisible(
      find.byKey(const Key('message')),
      250,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const Key('message')), 'hi');
    await t.scrollUntilVisible(
      find.text('Send'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.tap(find.text('Send'));
    await t.pump();
    await t.pumpWidget(const SizedBox());
    await t.pump();
    b.pending.complete(const CommandReply(id: 'run'));
    await t.pumpAndSettle();
    expect(t.takeException(), isNull);
    expect(b.commands.whereType<CancelRun>(), isEmpty);
    expect(b.closed, isFalse);
  });
  testWidgets('approval_shows_exact_server_tool_arguments', (t) async {
    t.view.physicalSize = const Size(320, 1200);
    t.view.devicePixelRatio = 1;
    addTearDown(t.view.resetPhysicalSize);
    addTearDown(t.view.resetDevicePixelRatio);
    final b = UiBackend();
    await t.pumpWidget(
      MaterialApp(
        home: ChatScreen(backend: b, conversationId: 'c'),
      ),
    );
    await t.pumpAndSettle();
    await t.scrollUntilVisible(
      find.text('Tool write • s • proposed'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.tap(find.text('Tool write • s • proposed'));
    await t.pumpAndSettle();
    expect(find.text(binding.argumentsJson), findsOneWidget);
    expect(find.text('Destination: https://example.test/mcp'), findsOneWidget);
    await t.scrollUntilVisible(
      find.text('Approve exact arguments'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.tap(find.text('Approve exact arguments'));
    await t.pumpAndSettle();
    expect(
      identical(b.commands.whereType<Approve>().single.binding, binding),
      isTrue,
    );
    expect(t.takeException(), isNull);
  });
  testWidgets('stream_stop_unknown_outcome', (t) async {
    final b = UiBackend();
    b.current = AppSnapshot(
      conversations: [conversation],
      runs: [run(state: AttemptState.outcomeUnknown)],
    );
    await t.pumpWidget(
      MaterialApp(
        home: ChatScreen(backend: b, conversationId: 'c'),
      ),
    );
    await t.pumpAndSettle();
    expect(find.textContaining('never retry'), findsWidgets);
    expect(find.text('Retry'), findsNothing);
    expect(find.text('Resume'), findsNothing);
    expect(find.text('Streaming text'), findsOneWidget);
    await t.scrollUntilVisible(
      find.text('Stop'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.tap(find.text('Stop'));
    await t.pumpAndSettle();
    expect(b.commands.whereType<CancelRun>().single.runId, 'r');
  });
  testWidgets('context_choice_is_explicit', (t) async {
    final b = UiBackend();
    await t.pumpWidget(
      MaterialApp(
        home: ChatScreen(backend: b, conversationId: 'c'),
      ),
    );
    await t.pumpAndSettle();
    expect(b.commands.whereType<ApplyContextSelection>(), isEmpty);
    await t.scrollUntilVisible(
      find.text('Choose context'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.tap(find.text('Choose context'));
    await t.pumpAndSettle();
    await t.tap(find.text('user: hello'));
    await t.tap(find.text('Apply selection'));
    await t.pumpAndSettle();
    expect(
      b.commands.whereType<ApplyContextSelection>().single.selection.messageIds,
      ['m'],
    );
    await t.scrollUntilVisible(
      find.text('Acknowledge inference disclosure'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.tap(find.text('Acknowledge inference disclosure'));
    await t.pumpAndSettle();
    expect(
      b.commands.whereType<AcknowledgeContextDisclosure>().single.origin,
      'https://example.test',
    );
  });
  testWidgets('service_busy_and_stale_approval_are_actionable', (t) async {
    final b = UiBackend()..failure = const BackendFailure(FailureCode.conflict);
    await t.pumpWidget(
      MaterialApp(
        home: ChatScreen(backend: b, conversationId: 'c'),
      ),
    );
    await t.pumpAndSettle();
    await t.scrollUntilVisible(
      find.text('Tool write • s • proposed'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.tap(find.text('Tool write • s • proposed'));
    await t.pumpAndSettle();
    await t.scrollUntilVisible(
      find.text('Approve exact arguments'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.tap(find.text('Approve exact arguments'));
    await t.pumpAndSettle();
    expect(find.textContaining('stale approval'), findsOneWidget);
    await t.scrollUntilVisible(
      find.byKey(const Key('message')),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const Key('message')), 'send');
    await t.scrollUntilVisible(
      find.text('Send'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.tap(find.text('Send'));
    await t.pumpAndSettle();
    expect(find.textContaining('busy'), findsOneWidget);
  });
  testWidgets('long_arguments_remain_scrollable', (t) async {
    t.view.physicalSize = const Size(320, 640);
    t.view.devicePixelRatio = 1;
    addTearDown(t.view.resetPhysicalSize);
    addTearDown(t.view.resetDevicePixelRatio);
    final b = UiBackend();
    final args = '{"long":"${'a' * 12000}"}';
    b.current = AppSnapshot(
      conversations: [conversation],
      runs: [run(arguments: args)],
    );
    await t.pumpWidget(
      MaterialApp(
        home: ChatScreen(backend: b, conversationId: 'c'),
      ),
    );
    await t.pumpAndSettle();
    await t.scrollUntilVisible(
      find.text('Tool write • s • proposed'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.tap(find.text('Tool write • s • proposed'));
    await t.pumpAndSettle();
    expect(find.text(args), findsOneWidget);
    expect(t.takeException(), isNull);
  });
}
