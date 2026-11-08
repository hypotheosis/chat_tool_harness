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
  inferenceOrigin: 'https://new.test',
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
  await t.ensureVisible(find.byKey(Key(key)));
  await t.pumpAndSettle();
  await t.tap(find.byKey(Key(key)));
  await t.pumpAndSettle();
}

void main() {
  for (final state in [RunState.paused, RunState.interrupted]) {
    testWidgets(
      'R1 older $state A collapses while new work B asks for its origin',
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
        expect(find.byKey(const Key('run-card-A')), findsOneWidget);
        expect(find.byKey(const Key('run-card-B')), findsNothing);
        expect(find.text('Messages will be sent to new.test.'), findsOneWidget);
        await tapKey(t, 'ack-new-work');
        expect(
          b.commands.whereType<AcknowledgeContextDisclosure>().single.origin,
          profileB.inferenceOrigin,
        );
        await t.tap(find.byTooltip('Summarize context'));
        await t.pumpAndSettle();
        expect(
          find.text('Selected content will be sent to new.test.'),
          findsOneWidget,
        );
        await t.tap(find.text('user: hello'));
        await t.tap(find.text('Request summary'));
        await t.pumpAndSettle();
        expect(
          b.commands.whereType<SummarizeContext>().single.selection.messageIds,
          ['m'],
        );
        // A is older than B, so it collapses to one line without an action.
        expect(find.byKey(const Key('resume-A')), findsNothing);
        expect(b.commands.whereType<ResumeRun>(), isEmpty);
      },
    );
  }
  testWidgets(
    'R2 older waiting A keeps bound approval resume and Stop after completed B',
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
      await t.ensureVisible(find.text('Approve exact arguments'));
      await t.pumpAndSettle();
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
      await tapKey(t, 'stop-A');
      await t.tap(find.byKey(const Key('confirm-stop')));
      await t.pumpAndSettle();
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
        find.byKey(const Key('run-card-A')),
        180,
        scrollable: find.byType(Scrollable).first,
      );
      expect(find.byKey(const Key('resume-A')), findsNothing);
      expect(find.text('Resume'), findsNothing);
      // The older run keeps its unknown-outcome warning, with no action.
      expect(
        find.textContaining("A tool call's outcome is unknown"),
        findsOneWidget,
      );
      expect(find.byKey(const Key('stop-A')), findsNothing);
      expect(b.commands.whereType<ResumeRun>(), isEmpty);
    });
  }
}
