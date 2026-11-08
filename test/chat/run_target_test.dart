import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/chat/chat_screen.dart';
import '../support/ui_fixture.dart';

const endpointB = EndpointConfig(
  id: 'b',
  name: 'B',
  baseUrl: 'https://new.test/v2/',
  model: 'b',
  contextTokens: 4096,
  maxOutputTokens: 512,
  temperature: 0,
  allowHttp: false,
);
const profileB = ProfileSnapshot(
  profileId: 'p',
  revision: 2,
  name: 'Updated profile',
  endpoint: endpointB,
  servers: [],
  instructions: '',
  limits: RunLimits(),
  inferenceDestination: 'https://new.test/v2/chat/completions',
);
const conversationB = Conversation(
  id: 'c',
  profile: profileB,
  messages: [Message(id: 'm', role: 'user', content: 'hello', calls: [])],
  summaries: [],
);
Run targetRun(
  String id,
  RunState state,
  ProfileSnapshot profile, {
  List<Attempt> attempts = const [],
}) => Run(
  id: id,
  conversationId: 'c',
  profile: profile,
  state: state,
  inferenceCount: 1,
  attemptCount: attempts.length,
  activeMs: 0,
  partialText: '',
  attempts: attempts,
  context: const RunContext(purpose: ContextPurpose.turn),
);
Future<void> tapKey(WidgetTester t, String key) async {
  await t.scrollUntilVisible(
    find.byKey(Key(key)),
    180,
    scrollable: find.byType(Scrollable).first,
  );
  await t.tap(find.byKey(Key(key)));
  await t.pumpAndSettle();
}

void main() {
  for (final state in [RunState.paused, RunState.interrupted]) {
    testWidgets(
      'R1 displayed disclosure matches explicit $state A versus new work B',
      (t) async {
        final b = UiBackend()
          ..current = AppSnapshot(
            conversations: [conversationB],
            runs: [
              targetRun('A', state, profileSnapshot),
              targetRun('B', RunState.completed, profileB),
            ],
          );
        await t.pumpWidget(
          MaterialApp(
            home: ChatScreen(backend: b, conversationId: 'c'),
          ),
        );
        await t.pumpAndSettle();
        await tapKey(t, 'ack-A');
        expect(
          find.text(
            'Run A disclosure • revision 1\nDestination: ${profileSnapshot.inferenceDestination}',
          ),
          findsOneWidget,
        );
        var ack = b.commands.whereType<AcknowledgeContextDisclosure>().single;
        expect(ack.snapshotRevision, 1);
        expect(ack.destination, profileSnapshot.inferenceDestination);
        await tapKey(t, 'ack-new-work');
        expect(
          find.text(
            'New turn / summary disclosure • revision 2\nDestination: ${profileB.inferenceDestination}',
          ),
          findsOneWidget,
        );
        ack = b.commands.whereType<AcknowledgeContextDisclosure>().last;
        expect(ack.snapshotRevision, 2);
        expect(ack.destination, profileB.inferenceDestination);
        await tapKey(t, 'summary-context');
        expect(
          find.text(
            'New summary disclosure • revision 2\nDestination: ${profileB.inferenceDestination}',
          ),
          findsOneWidget,
        );
        await t.tap(find.text('user: hello'));
        await t.tap(find.text('Request summary'));
        await t.pumpAndSettle();
        expect(
          b.commands.whereType<SummarizeContext>().single.selection.messageIds,
          ['m'],
        );
        expect(b.commands.whereType<ResumeRun>(), isEmpty);
      },
    );
  }
  testWidgets(
    'R2 older waiting A keeps bound approval resume context and Stop after completed B',
    (t) async {
      final b = UiBackend()
        ..current = AppSnapshot(
          conversations: [conversationB],
          runs: [
            targetRun(
              'A',
              RunState.waitingApproval,
              profileSnapshot,
              attempts: run().attempts,
            ),
            targetRun('B', RunState.completed, profileB),
          ],
        );
      await t.pumpWidget(
        MaterialApp(
          home: ChatScreen(backend: b, conversationId: 'c'),
        ),
      );
      await t.pumpAndSettle();
      await t.scrollUntilVisible(
        find.text('Tool write • s • proposed'),
        180,
        scrollable: find.byType(Scrollable).first,
      );
      await t.tap(find.text('Tool write • s • proposed'));
      await t.pumpAndSettle();
      await t.scrollUntilVisible(
        find.text('Approve exact arguments'),
        180,
        scrollable: find.byType(Scrollable).first,
      );
      await t.tap(find.text('Approve exact arguments'));
      await t.pumpAndSettle();
      expect(
        identical(b.commands.whereType<Approve>().single.binding, binding),
        isTrue,
      );
      await t.tap(find.text('Deny'));
      await t.pumpAndSettle();
      expect(b.commands.whereType<Deny>().single.attemptId, 'a');
      await tapKey(t, 'resume-A');
      expect(b.commands.whereType<ResumeRun>().single.runId, 'A');
      await tapKey(t, 'context-A');
      await t.tap(find.text('user: hello'));
      await t.tap(find.text('Apply selection'));
      await t.pumpAndSettle();
      expect(b.commands.whereType<ApplyContextSelection>().single.runId, 'A');
      await tapKey(t, 'stop-A');
      expect(b.commands.whereType<CancelRun>().single.runId, 'A');
      expect(find.byKey(const Key('resume-B')), findsNothing);
    },
  );
  for (final state in [AttemptState.outcomeUnknown, AttemptState.dispatching]) {
    testWidgets('R2 $state has no Resume even when older', (t) async {
      final b = UiBackend()
        ..current = AppSnapshot(
          conversations: [conversationB],
          runs: [
            targetRun(
              'A',
              RunState.interrupted,
              profileSnapshot,
              attempts: run(state: state).attempts,
            ),
            targetRun('B', RunState.completed, profileB),
          ],
        );
      await t.pumpWidget(
        MaterialApp(
          home: ChatScreen(backend: b, conversationId: 'c'),
        ),
      );
      await t.pumpAndSettle();
      await t.scrollUntilVisible(
        find.text('Run A • interrupted'),
        180,
        scrollable: find.byType(Scrollable).first,
      );
      expect(find.byKey(const Key('resume-A')), findsNothing);
      expect(b.commands.whereType<ResumeRun>(), isEmpty);
    });
  }
}
