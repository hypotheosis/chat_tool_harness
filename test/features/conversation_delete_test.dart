import 'dart:async';
import 'package:chat_tool_harness/app.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/chat/chat_screen.dart';
import 'package:chat_tool_harness/features/conversation_delete.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import '../support/ui_fixture.dart' as ui;

/// Removes conversation `c` when deletion succeeds, as the backend does.
class DeleteBackend extends ui.UiBackend {
  BackendFailure? deleteFailure;
  @override
  Future<CommandReply> execute(AppCommand command) async {
    if (command is DeleteConversation) {
      commands.add(command);
      if (deleteFailure case final error?) throw error;
      current = AppSnapshot(
        sequence: current.sequence + 1,
        endpoints: current.endpoints,
        servers: current.servers,
        profiles: current.profiles,
        runs: [
          for (final r in current.runs)
            if (r.conversationId != command.conversationId) r,
        ],
      );
      return const CommandReply();
    }
    return super.execute(command);
  }
}

Iterable<DeleteConversation> deletes(DeleteBackend b) =>
    b.commands.whereType<DeleteConversation>();

/// Pushes the chat screen for `c` over a plain host route, so a pop after
/// deletion lands on a real route.
Future<void> pumpChat(WidgetTester t, DeleteBackend b) async {
  await t.pumpWidget(
    MaterialApp(
      home: Builder(
        builder: (context) => Scaffold(
          body: TextButton(
            onPressed: () => Navigator.push(
              context,
              MaterialPageRoute<void>(
                builder: (_) => ChatScreen(backend: b, conversationId: 'c'),
              ),
            ),
            child: const Text('Open chat'),
          ),
        ),
      ),
    ),
  );
  await t.tap(find.text('Open chat'));
  await t.pumpAndSettle();
  expect(find.byType(ChatScreen), findsOneWidget);
}

Future<void> openChatMenu(WidgetTester t) async {
  await t.tap(find.byKey(const Key('chat-more')));
  await t.pumpAndSettle();
}

/// Holds each `CancelRun` until the test completes its gate; a run whose
/// gate succeeds is then stored as cancelled, as the backend does.
class StopBackend extends DeleteBackend {
  StopBackend() {
    runs = [active('r1'), active('r2')];
  }
  final gates = <Completer<void>>[];
  @override
  Future<CommandReply> execute(AppCommand command) async {
    if (command is CancelRun) {
      commands.add(command);
      final gate = Completer<void>();
      gates.add(gate);
      await gate.future;
      runs = [
        for (final r in current.runs)
          if (r.id == command.runId)
            ui.turnRun(id: r.id, state: RunState.cancelled)
          else
            r,
      ];
      return const CommandReply();
    }
    return super.execute(command);
  }

  List<String> get cancels => [
    for (final c in commands.whereType<CancelRun>()) c.runId,
  ];
}

Run active(String id) => ui.turnRun(id: id, state: RunState.waitingApproval);

/// One place that offers Stop and wait and Delete for conversation `c`.
class Surface {
  const Surface(
    this.name, {
    required this.pump,
    required this.startStop,
    required this.actionsEnabled,
    required this.openActions,
    required this.deleteEnabled,
  });
  final String name;
  final Future<void> Function(WidgetTester, DeleteBackend) pump;

  /// Opens the actions and taps Stop and wait, without settling.
  final Future<void> Function(WidgetTester) startStop;

  /// Whether the actions can be opened at all.
  final bool Function(WidgetTester) actionsEnabled;
  final Future<void> Function(WidgetTester) openActions;

  /// Whether Delete is enabled in the open actions.
  final bool Function(WidgetTester) deleteEnabled;
}

final surfaces = [
  Surface(
    'Conversations list',
    pump: (t, b) async {
      await t.pumpWidget(AgentApp(backend: b));
      await t.pumpAndSettle();
    },
    startStop: (t) async {
      await t.longPress(find.textContaining('Old profile'));
      await t.pumpAndSettle();
      await t.tap(find.text('Stop and wait'));
      await t.pump();
    },
    actionsEnabled: (t) =>
        t
            .widget<IconButton>(
              find.widgetWithIcon(IconButton, Icons.more_vert),
            )
            .onPressed !=
        null,
    openActions: (t) async {
      await t.tap(find.byTooltip('Conversation actions'));
      await t.pumpAndSettle();
    },
    deleteEnabled: (t) => t
        .widget<ListTile>(
          find.ancestor(
            of: find.text('Delete'),
            matching: find.byType(ListTile),
          ),
        )
        .enabled,
  ),
  Surface(
    'Chat screen',
    pump: pumpChat,
    startStop: (t) async {
      await openChatMenu(t);
      await t.tap(find.text('Stop and wait'));
      await t.pump();
    },
    actionsEnabled: (t) => t
        .widget<PopupMenuButton<void Function()>>(
          find.byKey(const Key('chat-more')),
        )
        .enabled,
    openActions: openChatMenu,
    deleteEnabled: (t) => t
        .widget<PopupMenuItem<void Function()>>(
          find.ancestor(
            of: find.text('Delete conversation'),
            matching: find.byType(PopupMenuItem<void Function()>),
          ),
        )
        .enabled,
  ),
];

void main() {
  group('rules read from the snapshot', () {
    test('runs to stop are the running and approval-waiting ones of the '
        'conversation', () {
      final runs = [
        ui.turnRun(id: 'running', state: RunState.running),
        ui.turnRun(id: 'waiting', state: RunState.waitingApproval),
        ui.turnRun(id: 'paused', state: RunState.paused),
        ui.turnRun(id: 'done', state: RunState.completed),
      ];
      expect(runsToStop(runs, 'c').map((r) => r.id), ['running', 'waiting']);
      expect(runsToStop(runs, 'other'), isEmpty);
    });
    for (final state in AttemptState.values) {
      final unknown =
          state == AttemptState.dispatching ||
          state == AttemptState.outcomeUnknown;
      test('an attempt in $state is ${unknown ? '' : 'not '}an unknown '
          'outcome', () {
        final runs = [ui.run(runState: RunState.cancelled, state: state)];
        expect(hasUnknownOutcomes(runs, 'c'), unknown);
        expect(hasUnknownOutcomes(runs, 'other'), isFalse);
      });
    }
  });

  group('Conversations list', () {
    testWidgets('long-press offers Delete, which asks before deleting', (
      t,
    ) async {
      final b = DeleteBackend()..runs = [];
      await t.pumpWidget(AgentApp(backend: b));
      await t.pumpAndSettle();
      await t.longPress(find.textContaining('Old profile'));
      await t.pumpAndSettle();
      expect(find.text('Stop and wait'), findsNothing);
      await t.tap(find.text('Delete'));
      await t.pumpAndSettle();
      expect(find.text('Delete conversation permanently?'), findsOneWidget);
      expect(find.textContaining('Unknown tool outcomes'), findsNothing);
      await t.tap(find.text('Cancel'));
      await t.pumpAndSettle();
      expect(deletes(b), isEmpty);
      await t.longPress(find.textContaining('Old profile'));
      await t.pumpAndSettle();
      await t.tap(find.text('Delete'));
      await t.pumpAndSettle();
      await t.tap(find.text('Confirm delete'));
      await t.pumpAndSettle();
      expect(deletes(b).single.conversationId, 'c');
      expect(deletes(b).single.confirmedUnknown, isFalse);
      expect(find.textContaining('Old profile'), findsNothing);
      await t.pumpWidget(const SizedBox());
    });

    testWidgets('the overflow button warns of unknown tool outcomes and '
        'confirms them', (t) async {
      final b = DeleteBackend()
        ..runs = [
          ui.run(
            runState: RunState.cancelled,
            state: AttemptState.outcomeUnknown,
          ),
        ];
      await t.pumpWidget(AgentApp(backend: b));
      await t.pumpAndSettle();
      await t.tap(find.byTooltip('Conversation actions'));
      await t.pumpAndSettle();
      await t.tap(find.text('Delete'));
      await t.pumpAndSettle();
      expect(find.textContaining('Unknown tool outcomes remain'), findsOne);
      await t.tap(find.text('Confirm delete'));
      await t.pumpAndSettle();
      expect(deletes(b).single.confirmedUnknown, isTrue);
      await t.pumpWidget(const SizedBox());
    });

    testWidgets('active work disables Delete and offers Stop and wait', (
      t,
    ) async {
      final b = DeleteBackend()..runs = [ui.run()];
      await t.pumpWidget(AgentApp(backend: b));
      await t.pumpAndSettle();
      await t.longPress(find.textContaining('Old profile'));
      await t.pumpAndSettle();
      final delete = t.widget<ListTile>(
        find.ancestor(of: find.text('Delete'), matching: find.byType(ListTile)),
      );
      expect(delete.enabled, isFalse);
      await t.tap(find.text('Delete'));
      await t.pumpAndSettle();
      expect(find.text('Delete conversation permanently?'), findsNothing);
      await t.tap(find.text('Stop and wait'));
      await t.pumpAndSettle();
      expect(b.commands.whereType<CancelRun>().single.runId, 'r');
      expect(deletes(b), isEmpty);
      await t.pumpWidget(const SizedBox());
    });

    testWidgets('a refused delete shows why and keeps the conversation', (
      t,
    ) async {
      final b = DeleteBackend()
        ..runs = []
        ..deleteFailure = const BackendFailure(FailureCode.conflict);
      await t.pumpWidget(AgentApp(backend: b));
      await t.pumpAndSettle();
      await t.longPress(find.textContaining('Old profile'));
      await t.pumpAndSettle();
      await t.tap(find.text('Delete'));
      await t.pumpAndSettle();
      await t.tap(find.text('Confirm delete'));
      await t.pumpAndSettle();
      expect(find.textContaining('Something changed or is busy'), findsOne);
      expect(find.textContaining('Old profile'), findsOneWidget);
      await t.pumpWidget(const SizedBox());
    });
  });

  group('Chat screen', () {
    testWidgets('the app bar menu deletes after confirmation and leaves the '
        'chat', (t) async {
      final b = DeleteBackend()..runs = [];
      await pumpChat(t, b);
      await openChatMenu(t);
      expect(find.text('Stop and wait'), findsNothing);
      await t.tap(find.text('Delete conversation'));
      await t.pumpAndSettle();
      await t.tap(find.text('Cancel'));
      await t.pumpAndSettle();
      expect(deletes(b), isEmpty);
      expect(find.byType(ChatScreen), findsOneWidget);
      await openChatMenu(t);
      await t.tap(find.text('Delete conversation'));
      await t.pumpAndSettle();
      expect(find.textContaining('Unknown tool outcomes'), findsNothing);
      await t.tap(find.text('Confirm delete'));
      await t.pumpAndSettle();
      expect(deletes(b).single.confirmedUnknown, isFalse);
      expect(find.byType(ChatScreen), findsNothing);
      expect(find.text('Open chat'), findsOneWidget);
      await t.pumpWidget(const SizedBox());
    });

    testWidgets('unknown tool outcomes are warned of and confirmed', (t) async {
      final b = DeleteBackend()
        ..runs = [
          ui.run(runState: RunState.failed, state: AttemptState.dispatching),
        ];
      await pumpChat(t, b);
      await openChatMenu(t);
      await t.tap(find.text('Delete conversation'));
      await t.pumpAndSettle();
      expect(find.textContaining('Unknown tool outcomes remain'), findsOne);
      await t.tap(find.text('Confirm delete'));
      await t.pumpAndSettle();
      expect(deletes(b).single.confirmedUnknown, isTrue);
      await t.pumpWidget(const SizedBox());
    });

    testWidgets('active work disables Delete and offers Stop and wait', (
      t,
    ) async {
      // Waiting for approval still holds work open; a running run is
      // covered by runsToStop above.
      final b = DeleteBackend()..runs = [ui.run()];
      await pumpChat(t, b);
      await openChatMenu(t);
      final delete = t.widget<PopupMenuItem<void Function()>>(
        find.ancestor(
          of: find.text('Delete conversation'),
          matching: find.byType(PopupMenuItem<void Function()>),
        ),
      );
      expect(delete.enabled, isFalse);
      await t.tap(find.text('Stop and wait'));
      await t.pumpAndSettle();
      expect(b.commands.whereType<CancelRun>().single.runId, 'r');
      expect(deletes(b), isEmpty);
      expect(find.byType(ChatScreen), findsOneWidget);
      await t.pumpWidget(const SizedBox());
    });

    testWidgets('a refused delete stays on the chat and shows why', (t) async {
      final b = DeleteBackend()
        ..runs = []
        ..deleteFailure = const BackendFailure(FailureCode.conflict);
      await pumpChat(t, b);
      await openChatMenu(t);
      await t.tap(find.text('Delete conversation'));
      await t.pumpAndSettle();
      await t.tap(find.text('Confirm delete'));
      await t.pumpAndSettle();
      expect(deletes(b), hasLength(1));
      expect(find.byType(ChatScreen), findsOneWidget);
      expect(find.textContaining('Something changed or is busy'), findsOne);
      await t.pumpWidget(const SizedBox());
    });
  });

  for (final surface in surfaces) {
    group('${surface.name} Stop and wait', () {
      testWidgets('stops runs one at a time and offers Delete only after the '
          'last one stopped', (t) async {
        final b = StopBackend();
        await surface.pump(t, b);
        await surface.startStop(t);
        expect(b.cancels, ['r1']);
        // Nothing can be deleted, or even opened, while a stop is pending.
        expect(surface.actionsEnabled(t), isFalse);
        b.gates[0].complete();
        await t.pump();
        await t.pump();
        expect(b.cancels, ['r1', 'r2']);
        expect(surface.actionsEnabled(t), isFalse);
        expect(deletes(b), isEmpty);
        b.gates[1].complete();
        await t.pumpAndSettle();
        expect(surface.actionsEnabled(t), isTrue);
        await surface.openActions(t);
        expect(find.text('Stop and wait'), findsNothing);
        expect(surface.deleteEnabled(t), isTrue);
        expect(deletes(b), isEmpty);
        await t.pumpWidget(const SizedBox());
      });

      testWidgets('a failed stop shows why and stops no further runs', (
        t,
      ) async {
        final b = StopBackend();
        await surface.pump(t, b);
        await surface.startStop(t);
        b.gates[0].completeError(const BackendFailure(FailureCode.conflict));
        await t.pumpAndSettle();
        expect(b.cancels, ['r1']);
        expect(find.textContaining('Something changed or is busy'), findsOne);
        await surface.openActions(t);
        expect(find.text('Stop and wait'), findsOneWidget);
        expect(surface.deleteEnabled(t), isFalse);
        expect(deletes(b), isEmpty);
        await t.pumpWidget(const SizedBox());
      });
    });
  }
}
