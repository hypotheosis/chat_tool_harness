import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/chat/chat_screen.dart';
import 'package:chat_tool_harness/features/profiles/profile_screen.dart';
import 'package:chat_tool_harness/features/screen_controller.dart';
import 'package:chat_tool_harness/features/settings/endpoint_screen.dart';
import 'package:chat_tool_harness/features/settings/server_screen.dart';
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

Future<void> pumpChat(WidgetTester t, UiBackend b) async {
  await t.pumpWidget(
    MaterialApp(
      key: UniqueKey(),
      home: ChatScreen(backend: b, conversationId: 'c'),
    ),
  );
  await t.pumpAndSettle();
}

Finder inCard(String runId, Finder finder) =>
    find.descendant(of: find.byKey(Key('run-card-$runId')), matching: finder);

const toolsAuth = FailureDetail(
  targetKind: 'mcpServer',
  targetName: 'Server',
  step: 'initialize',
  url: 'https://example.test/mcp',
  status: 401,
  reason: 'unknown API key',
);

/// A conversation whose endpoint host changed after it was acknowledged.
const movedProfile = ProfileSnapshot(
  inferenceOrigin: 'https://new.example.com',
  profileId: 'p',
  revision: 2,
  name: 'Old profile',
  endpoint: endpoint,
  servers: [],
  instructions: '',
  limits: RunLimits(),
);

/// Sending pauses for disclosure: the backend refreshes the snapshot to the
/// moved origin and records a paused run, as the Rust coordinator does.
class MovedOriginBackend extends UiBackend {
  @override
  Future<CommandReply> execute(AppCommand command) async {
    if (command is StartTurn) {
      setConversation(profile: movedProfile, acknowledged: false);
      runs = [
        turnRun(
          id: 'moved',
          state: RunState.paused,
          reason: ContextChoiceReason.disclosureRequired,
          profile: movedProfile,
        ),
      ];
    }
    if (command is AcknowledgeContextDisclosure) {
      setConversation(profile: movedProfile, acknowledged: true);
    }
    return super.execute(command);
  }
}

void main() {
  final actionCases = <String, (FailureCode, FailureDetail?, String, Type)>{
    'authentication on a server': (
      FailureCode.authentication,
      toolsAuth,
      'Open Server settings',
      ServerScreen,
    ),
    'missing credential': (
      FailureCode.credentialMissing,
      const FailureDetail(
        targetKind: 'credential',
        step: 'resolveCredential',
        reason: 'credential is missing',
      ),
      'Re-enter credential',
      EndpointScreen,
    ),
    'unreachable endpoint': (
      FailureCode.unavailableTransport,
      const FailureDetail(
        targetKind: 'inference',
        targetName: 'Endpoint',
        step: 'chatRequest',
        url: 'https://example.test/v1/chat/completions',
        reason: 'connection refused',
      ),
      'Open Endpoint settings',
      EndpointScreen,
    ),
    'protocol without detail': (
      FailureCode.protocol,
      null,
      'Open profile settings',
      ProfileScreen,
    ),
    'profile configuration': (
      FailureCode.configuration,
      const FailureDetail(
        targetKind: 'profile',
        targetName: 'Profile',
        step: 'refreshProfile',
      ),
      'Open Profile settings',
      ProfileScreen,
    ),
  };
  for (final MapEntry(key: name, value: (code, detail, label, screen))
      in actionCases.entries) {
    testWidgets('failed run card for $name opens $screen', (t) async {
      final b = UiBackend()..runs = [failedRun(code: code, detail: detail)];
      await pumpChat(t, b);
      expect(inCard('r', find.text('Run failed')), findsOneWidget);
      expect(
        inCard(
          'r',
          find.text(failureMessage(BackendFailure(code, detail: detail))),
        ),
        findsOneWidget,
      );
      expect(inCard('r', find.byType(FilledButton)), findsOneWidget);
      await t.tap(inCard('r', find.text(label)));
      await t.pumpAndSettle();
      expect(find.byType(screen), findsOneWidget);
    });
  }
  testWidgets('failed run shows one card with cause and settings action', (
    t,
  ) async {
    final b = UiBackend()
      ..runs = [failedRun(code: FailureCode.authentication, detail: toolsAuth)];
    await pumpChat(t, b);
    expect(find.textContaining('MCP server "Server"'), findsOneWidget);
    expect(find.text('Open Server settings'), findsOneWidget);
    expect(find.byKey(const Key('run-card-r')), findsOneWidget);
    expect(find.textContaining('revision'), findsNothing);
    expect(find.text('Apply current profile'), findsNothing);
    expect(find.text('Repair configuration'), findsNothing);
    expect(find.textContaining('immutable'), findsNothing);
    expect(find.textContaining('Run r'), findsNothing);
    await t.tap(inCard('r', find.text('Details')));
    await t.pumpAndSettle();
    final details = t
        .widget<SelectableText>(find.byKey(const Key('run-details-r')))
        .data!;
    expect(details, contains('Run id: r'));
    expect(details, contains('Target: MCP server "Server"'));
    expect(details, contains('Step: initialization'));
    expect(details, contains('URL: https://example.test/mcp'));
    expect(details, contains('HTTP status: 401'));
    expect(details, contains('Reason: unknown API key'));
  });
  testWidgets('context length failure offers the context chooser', (t) async {
    final b = UiBackend()..runs = [failedRun(code: FailureCode.contextLength)];
    await pumpChat(t, b);
    await t.tap(inCard('r', find.text('Choose context')));
    await t.pumpAndSettle();
    expect(find.text('Explicit context selection'), findsOneWidget);
  });
  for (final code in [FailureCode.rateLimited, FailureCode.limit]) {
    testWidgets('$code failure retries the triggering message', (t) async {
      final b = UiBackend()..runs = [failedRun(code: code)];
      await pumpChat(t, b);
      await t.tap(inCard('r', find.text('Retry')));
      await t.pumpAndSettle();
      final turn = b.commands.whereType<StartTurn>().single;
      expect((turn.conversationId, turn.text), ('c', 'hello'));
    });
  }
  testWidgets('a paused run with a retryable failure offers Resume', (t) async {
    final b = UiBackend()
      ..runs = [turnRun(state: RunState.paused, code: FailureCode.rateLimited)];
    await pumpChat(t, b);
    expect(inCard('r', find.text('Paused')), findsOneWidget);
    expect(inCard('r', find.text('Retry')), findsNothing);
    await t.tap(inCard('r', find.text('Resume')));
    await t.pumpAndSettle();
    expect(b.commands.whereType<ResumeRun>().single.runId, 'r');
  });
  testWidgets('a run card sits under the turn that started it', (t) async {
    final b = UiBackend()
      ..current = AppSnapshot(
        conversations: [
          Conversation(
            id: 'c',
            profile: profileSnapshot,
            originAcknowledged: true,
            summaries: const [],
            messages: const [
              Message(id: 'm', role: 'user', content: 'first', calls: []),
              Message(id: 'm2', role: 'assistant', content: 'reply', calls: []),
              Message(id: 'm3', role: 'user', content: 'second', calls: []),
            ],
          ),
        ],
        runs: [
          turnRun(code: FailureCode.rateLimited),
          turnRun(id: 'r2', state: RunState.completed, inputMessageId: 'm3'),
        ],
      );
    await pumpChat(t, b);
    final card = t.getRect(find.byKey(const Key('run-card-r')));
    expect(card.top, greaterThan(t.getRect(find.text('reply')).bottom));
    expect(card.bottom, lessThan(t.getRect(find.text('second')).top));
  });
  for (final state in [AttemptState.outcomeUnknown, AttemptState.dispatching]) {
    testWidgets('failed conflict run with a $state attempt offers no Retry', (
      t,
    ) async {
      final b = UiBackend()
        ..runs = [
          turnRun(
            code: FailureCode.conflict,
            attempts: run(state: state).attempts,
          ),
        ];
      await pumpChat(t, b);
      expect(
        inCard(
          'r',
          find.text(
            "A tool call's outcome is unknown; check the remote system before continuing.",
          ),
        ),
        findsOneWidget,
      );
      expect(find.text('Retry'), findsNothing);
      expect(inCard('r', find.byType(FilledButton)), findsNothing);
      // Details is the card's only button, and it starts no work.
      await t.tap(inCard('r', find.text('Details')));
      await t.pumpAndSettle();
      expect(b.commands.whereType<StartTurn>(), isEmpty);
      expect(b.commands.whereType<ResumeRun>(), isEmpty);
    });
  }
  testWidgets('a disclosure pause with an unknown outcome offers Stop only', (
    t,
  ) async {
    final b = UiBackend()
      ..runs = [
        turnRun(
          state: RunState.paused,
          reason: ContextChoiceReason.disclosureRequired,
          profile: movedProfile,
          attempts: run(state: AttemptState.outcomeUnknown).attempts,
        ),
      ];
    await pumpChat(t, b);
    expect(inCard('r', find.text('Allow')), findsNothing);
    expect(inCard('r', find.text('Stop')), findsOneWidget);
  });
  testWidgets('a paused run offers Resume after its settings are opened', (
    t,
  ) async {
    final b = UiBackend()
      ..runs = [
        turnRun(
          state: RunState.paused,
          code: FailureCode.unavailableTransport,
          detail: const FailureDetail(
            targetKind: 'inference',
            targetName: 'Endpoint',
            step: 'chatRequest',
            reason: 'connection refused',
          ),
        ),
      ];
    await pumpChat(t, b);
    expect(inCard('r', find.text('Resume')), findsNothing);
    await t.tap(inCard('r', find.text('Open Endpoint settings')));
    await t.pumpAndSettle();
    expect(find.byType(EndpointScreen), findsOneWidget);
    await t.pageBack();
    // Returning refreshes the chat; let the stream cancellation it awaits
    // complete outside fake time.
    await t.runAsync(() => Future<void>.delayed(Duration.zero));
    await t.pumpAndSettle();
    expect(inCard('r', find.text('Open Endpoint settings')), findsNothing);
    expect(inCard('r', find.byType(FilledButton)), findsOneWidget);
    await t.tap(inCard('r', find.text('Resume')));
    await t.pumpAndSettle();
    expect(b.commands.whereType<ResumeRun>().single.runId, 'r');
  });
  testWidgets('a paused run offers Resume after a context selection', (
    t,
  ) async {
    final b = UiBackend()
      ..runs = [
        turnRun(state: RunState.paused, code: FailureCode.contextLength),
      ];
    await pumpChat(t, b);
    await t.tap(inCard('r', find.text('Choose context')));
    await t.pumpAndSettle();
    await t.tap(find.text('Cancel'));
    await t.pumpAndSettle();
    // Cancelling the chooser leaves the card asking for a choice.
    expect(inCard('r', find.text('Choose context')), findsOneWidget);
    await t.tap(inCard('r', find.text('Choose context')));
    await t.pumpAndSettle();
    await t.tap(find.text('user: hello'));
    await t.tap(find.text('Apply selection'));
    await t.pumpAndSettle();
    expect(b.commands.whereType<ApplyContextSelection>().single.runId, 'r');
    expect(inCard('r', find.text('Choose context')), findsNothing);
    await t.tap(inCard('r', find.text('Resume')));
    await t.pumpAndSettle();
    expect(b.commands.whereType<ResumeRun>().single.runId, 'r');
  });
  testWidgets('only the latest run gets a full card; earlier runs collapse', (
    t,
  ) async {
    final b = UiBackend()
      ..current = AppSnapshot(
        conversations: [
          Conversation(
            id: 'c',
            profile: profileSnapshot,
            originAcknowledged: true,
            summaries: const [],
            messages: const [
              Message(id: 'm', role: 'user', content: 'first', calls: []),
              Message(id: 'm2', role: 'user', content: 'second', calls: []),
              Message(id: 'm3', role: 'user', content: 'third', calls: []),
            ],
          ),
        ],
        runs: [
          turnRun(id: 'old', code: FailureCode.rateLimited),
          turnRun(
            id: 'unknown',
            code: FailureCode.conflict,
            inputMessageId: 'm2',
            attempts: run(state: AttemptState.outcomeUnknown).attempts,
          ),
          turnRun(
            id: 'new',
            code: FailureCode.rateLimited,
            inputMessageId: 'm3',
          ),
        ],
      );
    await pumpChat(t, b);
    final rate = failureMessage(const BackendFailure(FailureCode.rateLimited));
    expect(inCard('old', find.text('Run failed: $rate')), findsOneWidget);
    expect(
      inCard('old', find.byWidgetPredicate((w) => w is ButtonStyleButton)),
      findsOneWidget,
    );
    expect(inCard('old', find.text('Details')), findsOneWidget);
    expect(
      inCard(
        'unknown',
        find.text(
          "Run failed: A tool call's outcome is unknown; check the remote system before continuing.",
        ),
      ),
      findsOneWidget,
    );
    expect(inCard('unknown', find.byType(FilledButton)), findsNothing);
    expect(find.text('Retry'), findsOneWidget);
    expect(inCard('new', find.text('Retry')), findsOneWidget);
    await t.tap(inCard('old', find.text('Details')));
    await t.pumpAndSettle();
    expect(find.byKey(const Key('run-details-old')), findsOneWidget);
  });
  testWidgets('a failed summary run sits last and Retry reopens the chooser', (
    t,
  ) async {
    final b = UiBackend()
      ..current = AppSnapshot(
        conversations: [
          Conversation(
            id: 'c',
            profile: profileSnapshot,
            originAcknowledged: true,
            summaries: const [],
            messages: const [
              Message(id: 'm', role: 'user', content: 'first', calls: []),
              Message(id: 'm2', role: 'assistant', content: 'reply', calls: []),
            ],
          ),
        ],
        runs: [
          turnRun(id: 'turn', state: RunState.completed),
          turnRun(
            id: 'summary',
            code: FailureCode.rateLimited,
            inputMessageId: null,
          ),
        ],
      );
    await pumpChat(t, b);
    final card = t.getRect(find.byKey(const Key('run-card-summary')));
    expect(card.top, greaterThan(t.getRect(find.text('reply')).bottom));
    await t.tap(inCard('summary', find.text('Retry')));
    await t.pumpAndSettle();
    expect(find.text('Explicit context selection'), findsOneWidget);
    expect(find.text('Request summary'), findsOneWidget);
    expect(b.commands.whereType<StartTurn>(), isEmpty);
  });
  testWidgets('header shows profile and model and opens profile settings', (
    t,
  ) async {
    final b = UiBackend()..runs = [];
    await pumpChat(t, b);
    expect(find.text('Old profile'), findsOneWidget);
    expect(find.text('manual'), findsOneWidget);
    await t.tap(find.byTooltip('Profile settings'));
    await t.pumpAndSettle();
    expect(find.byType(ProfileScreen), findsOneWidget);
    expect(b.commands.whereType<ApplyProfile>(), isEmpty);
  });
  testWidgets('successful runs show no run row', (t) async {
    final b = UiBackend()..runs = [completedRun()];
    await pumpChat(t, b);
    expect(find.textContaining('Run '), findsNothing);
    expect(find.byKey(const Key('run-r')), findsNothing);
    expect(find.byKey(const Key('run-card-r')), findsNothing);
    expect(find.text('Working…'), findsNothing);
  });
  testWidgets('a running run is one Working row whose Stop confirms', (
    t,
  ) async {
    final b = UiBackend()..runs = [turnRun(state: RunState.running)];
    // The spinner never settles, so pump fixed frames instead.
    await t.pumpWidget(
      MaterialApp(
        home: ChatScreen(backend: b, conversationId: 'c'),
      ),
    );
    await t.pump();
    expect(find.text('Working…'), findsOneWidget);
    expect(find.textContaining('best effort'), findsNothing);
    await t.tap(find.byKey(const Key('stop-r')));
    await t.pump(const Duration(milliseconds: 500));
    expect(
      find.text(
        'Stop is local and best effort. Work already sent to a server is not rolled back.',
      ),
      findsOneWidget,
    );
    await t.tap(find.text('Cancel'));
    await t.pump(const Duration(milliseconds: 500));
    expect(b.commands.whereType<CancelRun>(), isEmpty);
    await t.tap(find.byKey(const Key('stop-r')));
    await t.pump(const Duration(milliseconds: 500));
    await t.tap(find.byKey(const Key('confirm-stop')));
    await t.pump(const Duration(milliseconds: 500));
    expect(b.commands.whereType<CancelRun>().single.runId, 'r');
  });
  testWidgets('disclosure card only for unacknowledged origin', (t) async {
    final b = UiBackend()
      ..runs = []
      ..originAcknowledged = false;
    await pumpChat(t, b);
    expect(find.text('Messages will be sent to example.test.'), findsOneWidget);
    final send = find.widgetWithText(FilledButton, 'Send');
    expect(t.widget<FilledButton>(send).onPressed, isNull);
    await t.tap(find.text('Allow'));
    await t.pumpAndSettle();
    expect(
      b.commands.whereType<AcknowledgeContextDisclosure>().single.origin,
      'https://example.test',
    );
    b.originAcknowledged = true;
    await pumpChat(t, b);
    expect(find.textContaining('Messages will be sent to'), findsNothing);
    expect(t.widget<FilledButton>(send).onPressed, isNotNull);
  });
  testWidgets('send after the origin moved pauses, then Allow resumes', (
    t,
  ) async {
    final b = MovedOriginBackend()..runs = [];
    await pumpChat(t, b);
    expect(find.textContaining('Messages will be sent to'), findsNothing);
    await t.enterText(find.byKey(const Key('message')), 'hello');
    await t.tap(find.text('Send'));
    await t.pumpAndSettle();
    expect(inCard('moved', find.text('Paused')), findsOneWidget);
    expect(
      inCard(
        'moved',
        find.textContaining('Messages will be sent to new.example.com'),
      ),
      findsOneWidget,
    );
    // The run card asks; the composer does not ask a second time.
    expect(find.text('Allow'), findsOneWidget);
    await t.tap(inCard('moved', find.text('Allow')));
    await t.pumpAndSettle();
    expect(
      b.commands.whereType<AcknowledgeContextDisclosure>().single.origin,
      'https://new.example.com',
    );
    expect(b.commands.whereType<ResumeRun>().single.runId, 'moved');
    expect(
      b.commands.indexWhere((c) => c is AcknowledgeContextDisclosure),
      lessThan(b.commands.indexWhere((c) => c is ResumeRun)),
    );
  });
  testWidgets('context choosers appear only when the backend asks', (t) async {
    final b = UiBackend()..runs = [];
    await pumpChat(t, b);
    expect(find.text('Choose context'), findsNothing);
    expect(find.text('Choose summary context'), findsNothing);
    b.runs = [
      turnRun(
        state: RunState.paused,
        reason: ContextChoiceReason.windowExceeded,
      ),
    ];
    await pumpChat(t, b);
    await t.tap(inCard('r', find.text('Choose context')));
    await t.pumpAndSettle();
    await t.tap(find.text('user: hello'));
    await t.tap(find.text('Apply selection'));
    await t.pumpAndSettle();
    expect(
      b.commands.whereType<ApplyContextSelection>().single.selection.messageIds,
      ['m'],
    );
  });
  testWidgets('composer and disclosure sit above a 48 px bottom inset', (
    t,
  ) async {
    const height = 640.0;
    const inset = 48.0;
    t.view.physicalSize = const Size(1080, height * 3);
    t.view.devicePixelRatio = 3;
    t.view.padding = const FakeViewPadding(bottom: inset * 3);
    t.view.viewPadding = const FakeViewPadding(bottom: inset * 3);
    addTearDown(t.view.reset);
    final b = UiBackend()
      ..runs = [failedRun(code: FailureCode.authentication, detail: toolsAuth)]
      ..originAcknowledged = false;
    await pumpChat(t, b);
    final limit = height - inset;
    for (final finder in [
      find.byKey(const Key('message')),
      find.widgetWithText(FilledButton, 'Send'),
      find.widgetWithText(FilledButton, 'Allow'),
    ]) {
      expect(t.getRect(finder).bottom, lessThanOrEqualTo(limit));
    }
    // The run cards scroll in a list that ends above the composer.
    final list = t.getRect(find.byType(Scrollable).first);
    expect(
      list.bottom,
      lessThanOrEqualTo(t.getRect(find.byKey(const Key('message'))).top),
    );
    expect(list.bottom, lessThanOrEqualTo(limit));
  });
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
    await t.tap(find.byKey(const Key('confirm-stop')));
    await t.pumpAndSettle();
    expect(b.commands.whereType<CancelRun>().single.runId, 'r');
  });
  testWidgets('context_choice_is_explicit', (t) async {
    final b = UiBackend()
      ..runs = [
        turnRun(state: RunState.paused, reason: ContextChoiceReason.rawLimit),
      ];
    await pumpChat(t, b);
    expect(b.commands.whereType<ApplyContextSelection>(), isEmpty);
    await t.tap(inCard('r', find.text('Choose context')));
    await t.pumpAndSettle();
    await t.tap(find.text('Apply selection'));
    await t.pumpAndSettle();
    expect(
      b.commands.whereType<ApplyContextSelection>().single.selection.messageIds,
      isEmpty,
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
