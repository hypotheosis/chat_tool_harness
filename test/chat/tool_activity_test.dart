import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/chat/chat_screen.dart';
import '../support/ui_fixture.dart';

ToolCall call(String id, String name, String args) => ToolCall(
  key: ToolKey(profileId: 'p', serverId: 's', name: name),
  providerCallId: id,
  argumentsJson: args,
);

Message msg(
  String id,
  String role,
  String content, {
  List<ToolCall> calls = const [],
  String? callId,
}) =>
    Message(id: id, role: role, content: content, calls: calls, callId: callId);

Future<void> pump(WidgetTester t, List<Message> messages) async {
  final b = UiBackend()
    ..current = AppSnapshot(
      conversations: [
        Conversation(
          id: 'c',
          profile: profileSnapshot,
          messages: messages,
          summaries: const [],
          originAcknowledged: true,
        ),
      ],
    );
  await t.pumpWidget(
    MaterialApp(
      home: ChatScreen(backend: b, conversationId: 'c'),
    ),
  );
  await t.pumpAndSettle();
}

void main() {
  testWidgets('tool calls and raw results collapse into one card', (t) async {
    await pump(t, [
      msg('u', 'user', 'What is arrowood'),
      msg(
        'a1',
        'assistant',
        'Let me look that up.',
        calls: [
          call('c1', 'search', '{"query":"arrowood"}'),
          call('c2', 'get_concept', '{"path":"/x"}'),
        ],
      ),
      msg('t1', 'tool', '{"hits":["RAW-SEARCH"]}', callId: 'c1'),
      msg('t2', 'tool', '{"body":"RAW-CONCEPT"}', callId: 'c2'),
      msg('a2', 'assistant', '', calls: [call('c3', 'search', '{}')]),
      msg('t3', 'tool', 'RAW-THIRD', callId: 'c3'),
      msg('f', 'assistant', 'Arrowood is a shrub.'),
    ]);
    // The conversation reads as user, assistant text, one tool row, reply.
    expect(find.text('What is arrowood'), findsOneWidget);
    expect(find.text('Let me look that up.'), findsOneWidget);
    expect(find.text('Arrowood is a shrub.'), findsOneWidget);
    expect(find.text('tool'), findsNothing);
    expect(find.textContaining('RAW-'), findsNothing);
    // The empty call-only turn after the text joins the same stretch.
    expect(find.byKey(const Key('tools-a1')), findsOneWidget);
    expect(find.byKey(const Key('tools-a2')), findsNothing);
    expect(find.text('Used 3 tools'), findsOneWidget);
    expect(find.text('search, get_concept'), findsOneWidget);
    // Expanding shows each call; expanding a call shows its exact result.
    await t.tap(find.text('Used 3 tools'));
    await t.pumpAndSettle();
    expect(find.text('search · Server'), findsNWidgets(2));
    await t.tap(find.text('get_concept · Server'));
    await t.pumpAndSettle();
    expect(find.text('{"path":"/x"}'), findsOneWidget);
    expect(find.text('{"body":"RAW-CONCEPT"}'), findsOneWidget);
    expect(find.textContaining('RAW-SEARCH'), findsNothing);
  });

  testWidgets('a user message ends a stretch of tool activity', (t) async {
    await pump(t, [
      msg('a1', 'assistant', '', calls: [call('c1', 'search', '{}')]),
      msg('t1', 'tool', 'one', callId: 'c1'),
      msg('u', 'user', 'and then?'),
      msg('t2', 'tool', 'orphan result'),
    ]);
    expect(find.text('Used 1 tool'), findsNWidgets(2));
    expect(find.byKey(const Key('tools-a1')), findsOneWidget);
    expect(find.byKey(const Key('tools-t2')), findsOneWidget);
    await t.tap(find.byKey(const Key('tools-t2')));
    await t.pumpAndSettle();
    expect(find.text('orphan result'), findsOneWidget);
  });
}
