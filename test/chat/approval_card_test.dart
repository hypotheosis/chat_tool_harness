import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/chat/approval_card.dart';
import 'package:chat_tool_harness/features/chat/chat_screen.dart';
import '../support/ui_fixture.dart';

const profileId = 'profile-6f1d0c2e-uuid';
const serverId = 'server-9a7b3c11-uuid';
const otherServerId = 'server-2c44e8d0-uuid';

const twoServers = ProfileSnapshot(
  inferenceOrigin: 'https://example.test',
  profileId: profileId,
  revision: 1,
  name: 'Profile',
  endpoint: endpoint,
  servers: [
    ServerConfig(
      id: serverId,
      name: 'Notes',
      url: 'https://notes.example.test/mcp',
      allowHttp: false,
    ),
    ServerConfig(
      id: otherServerId,
      name: 'Calendar',
      url: 'https://calendar.example.test/mcp',
      allowHttp: false,
    ),
  ],
  instructions: '',
  limits: RunLimits(),
);

ApprovalBinding bindingFor(String attempt, String? server, String tool) =>
    ApprovalBinding(
      attemptId: attempt,
      key: ToolKey(profileId: profileId, serverId: server, name: tool),
      providerCallId: 'call-$attempt',
      definitionDigest: 'definition',
      argumentsDigest: 'args-$attempt',
      argumentsJson: '{"text":"$attempt arguments"}',
      schemaJson: '{}',
      destinationDigest: 'destination',
    );

Attempt pendingAttempt(
  String id,
  String? server,
  String tool, {
  AttemptState state = AttemptState.proposed,
  bool bound = true,
}) {
  final b = bindingFor(id, server, tool);
  return Attempt(
    id: id,
    state: state,
    call: ToolCall(
      key: b.key,
      providerCallId: b.providerCallId,
      argumentsJson: b.argumentsJson,
    ),
    binding: bound ? b : null,
  );
}

final attemptOne = pendingAttempt('attempt-1111-uuid', serverId, 'search');
final attemptTwo = pendingAttempt('attempt-2222-uuid', serverId, 'write');

UiBackend backendWith(List<Attempt> attempts, {List<ServerConfig>? servers}) {
  final b = UiBackend();
  b.current = AppSnapshot(
    endpoints: [endpoint],
    servers: servers ?? twoServers.servers,
    conversations: [conversation],
    runs: [
      turnRun(
        state: RunState.waitingApproval,
        profile: twoServers,
        attempts: attempts,
      ),
    ],
  );
  return b;
}

Future<void> pumpChat(WidgetTester t, UiBackend b) async {
  t.view.physicalSize = const Size(600, 1600);
  t.view.devicePixelRatio = 1;
  addTearDown(t.view.resetPhysicalSize);
  addTearDown(t.view.resetDevicePixelRatio);
  await t.pumpWidget(
    MaterialApp(
      home: ChatScreen(backend: b, conversationId: 'c'),
    ),
  );
  await t.pumpAndSettle();
}

Future<void> tapKey(WidgetTester t, String key) async {
  await t.ensureVisible(find.byKey(Key(key)));
  await t.pumpAndSettle();
  await t.tap(find.byKey(Key(key)));
  await t.pumpAndSettle();
}

Finder inApproval(Finder finder) =>
    find.descendant(of: find.byKey(const Key('approval-r')), matching: finder);

/// Holds commands until the test releases them, to observe the loading state.
class HeldBackend extends UiBackend {
  final held = Completer<void>();
  @override
  Future<CommandReply> execute(AppCommand command) async {
    if (command is DecideApprovals) await held.future;
    return super.execute(command);
  }
}

void main() {
  testWidgets('the approval card lists every pending call by name', (t) async {
    final b = backendWith([attemptOne, attemptTwo]);
    await pumpChat(t, b);
    expect(find.byKey(const Key('approval-r')), findsOneWidget);
    expect(inApproval(find.text('Notes wants to run 2 tools')), findsOneWidget);
    expect(inApproval(find.text('search · Notes')), findsOneWidget);
    expect(inApproval(find.text('write · Notes')), findsOneWidget);
    expect(
      inApproval(find.textContaining('attempt-1111-uuid arguments')),
      findsOneWidget,
    );
    expect(
      inApproval(find.textContaining('attempt-2222-uuid arguments')),
      findsOneWidget,
    );
    // No ids anywhere in the card.
    for (final id in [profileId, serverId, 'attempt-1111-uuid"', 'call-']) {
      expect(inApproval(find.textContaining(id)), findsNothing);
    }
    // Arguments are pretty-printed.
    expect(
      inApproval(find.text('{\n  "text": "attempt-1111-uuid arguments"\n}')),
      findsOneWidget,
    );
  });

  testWidgets('long arguments stay collapsed until shown', (t) async {
    final long = ApprovalBinding(
      attemptId: 'attempt-7777-uuid',
      key: const ToolKey(profileId: profileId, serverId: serverId, name: 'put'),
      providerCallId: 'call-7',
      definitionDigest: 'definition',
      argumentsDigest: 'args-7',
      argumentsJson: '{"body":"${'x' * 1000}"}',
      schemaJson: '{}',
      destinationDigest: 'destination',
    );
    final b = backendWith([
      Attempt(
        id: long.attemptId,
        state: AttemptState.proposed,
        call: ToolCall(
          key: long.key,
          providerCallId: long.providerCallId,
          argumentsJson: long.argumentsJson,
        ),
        binding: long,
      ),
    ]);
    await pumpChat(t, b);
    expect(inApproval(find.textContaining('xxxxxxxx')), findsNothing);
    await t.tap(inApproval(find.text('Show arguments')));
    await t.pumpAndSettle();
    expect(inApproval(find.textContaining('xxxxxxxx')), findsOneWidget);
    expect(inApproval(find.text('Hide arguments')), findsOneWidget);
  });

  test('arguments are pretty-printed only when lossless', () {
    expect(prettyJson('{"a":1}'), '{\n  "a": 1\n}');
    // Above 2^63: Dart would read it as a double and change the digits.
    const big = '{"id":12345678901234567890}';
    expect(prettyJson(big), big);
    expect(prettyJson('not json'), 'not json');
  });

  testWidgets('a huge integer argument shows its exact digits', (t) async {
    final b = bindingFor('attempt-9999-uuid', serverId, 'get');
    final exact = ApprovalBinding(
      attemptId: b.attemptId,
      key: b.key,
      providerCallId: b.providerCallId,
      definitionDigest: b.definitionDigest,
      argumentsDigest: b.argumentsDigest,
      argumentsJson: '{"id":12345678901234567890}',
      schemaJson: b.schemaJson,
      destinationDigest: b.destinationDigest,
    );
    final backend = backendWith([
      Attempt(
        id: exact.attemptId,
        state: AttemptState.proposed,
        call: ToolCall(
          key: exact.key,
          providerCallId: exact.providerCallId,
          argumentsJson: exact.argumentsJson,
        ),
        binding: exact,
      ),
    ]);
    await pumpChat(t, backend);
    expect(
      inApproval(find.text('{"id":12345678901234567890}')),
      findsOneWidget,
    );
    expect(inApproval(find.textContaining('.0')), findsNothing);
  });

  testWidgets('decision actions sit above the list of calls', (t) async {
    final b = backendWith([attemptOne, attemptTwo]);
    await pumpChat(t, b);
    final approve = t.getRect(find.byKey(const Key('approve-all-r')));
    final first = t.getRect(inApproval(find.text('search · Notes')));
    expect(approve.bottom, lessThanOrEqualTo(first.top));
  });

  testWidgets('a per-call Deny names its tool and server', (t) async {
    final b = backendWith([attemptOne]);
    await pumpChat(t, b);
    expect(find.byTooltip('Deny search on Notes'), findsOneWidget);
  });

  testWidgets('expanded arguments stay with their call after a Deny', (
    t,
  ) async {
    Attempt longCall(String id) {
      final b = bindingFor(id, serverId, 'put');
      final long = ApprovalBinding(
        attemptId: id,
        key: b.key,
        providerCallId: b.providerCallId,
        definitionDigest: b.definitionDigest,
        argumentsDigest: b.argumentsDigest,
        argumentsJson: '{"body":"$id ${'x' * 400}"}',
        schemaJson: b.schemaJson,
        destinationDigest: b.destinationDigest,
      );
      return Attempt(
        id: id,
        state: AttemptState.proposed,
        call: ToolCall(
          key: long.key,
          providerCallId: long.providerCallId,
          argumentsJson: long.argumentsJson,
        ),
        binding: long,
      );
    }

    final one = longCall('attempt-a-uuid');
    final two = longCall('attempt-b-uuid');
    final b = backendWith([one, two]);
    await pumpChat(t, b);
    await t.tap(inApproval(find.text('Show arguments')).first);
    await t.pumpAndSettle();
    expect(inApproval(find.textContaining('attempt-a-uuid x')), findsOneWidget);
    // The first call is denied; the second must not inherit its expansion.
    b.runs = [
      turnRun(
        state: RunState.waitingApproval,
        profile: twoServers,
        attempts: [
          Attempt(
            id: one.id,
            state: AttemptState.denied,
            call: one.call,
            binding: one.binding,
          ),
          two,
        ],
      ),
    ];
    b.emit(AppEvent(sequence: b.current.sequence + 1, conversationId: 'c'));
    await t.pumpAndSettle();
    expect(inApproval(find.textContaining('attempt-b-uuid x')), findsNothing);
  });

  testWidgets('message cards name the tool and server, not the id', (t) async {
    final b = backendWith([]);
    b.current = AppSnapshot(
      endpoints: [endpoint],
      servers: twoServers.servers,
      conversations: [
        Conversation(
          id: 'c',
          profile: twoServers,
          messages: [
            const Message(id: 'm', role: 'user', content: 'hello', calls: []),
            Message(
              id: 'm2',
              role: 'assistant',
              content: '',
              calls: [attemptOne.call],
            ),
          ],
          summaries: const [],
          originAcknowledged: true,
        ),
      ],
    );
    await pumpChat(t, b);
    expect(find.text('search · Notes'), findsOneWidget);
    expect(find.textContaining(serverId), findsNothing);
  });

  testWidgets('calls of a server trusted while waiting need no decision', (
    t,
  ) async {
    final b = backendWith(
      [attemptOne, attemptTwo],
      servers: [
        for (final s in twoServers.servers)
          ServerConfig(
            id: s.id,
            name: s.name,
            url: s.url,
            allowHttp: s.allowHttp,
            trusted: s.id == serverId,
          ),
      ],
    );
    await pumpChat(t, b);
    expect(find.byKey(const Key('approval-r')), findsNothing);
    expect(find.text('search · Notes · Trusted, runs on resume'), findsOne);
    await tapKey(t, 'resume-r');
    expect(b.commands.whereType<ResumeRun>().single.runId, 'r');
    expect(b.commands.whereType<DecideApprovals>(), isEmpty);
  });

  testWidgets('only untrusted calls are listed when trust is mixed', (t) async {
    final b = backendWith(
      [attemptOne, pendingAttempt('attempt-3333-uuid', otherServerId, 'book')],
      servers: [
        for (final s in twoServers.servers)
          ServerConfig(
            id: s.id,
            name: s.name,
            url: s.url,
            allowHttp: s.allowHttp,
            trusted: s.id == serverId,
          ),
      ],
    );
    await pumpChat(t, b);
    expect(inApproval(find.text('Calendar wants to run 1 tool')), findsOne);
    expect(inApproval(find.text('search · Notes')), findsNothing);
    expect(find.byKey(const Key('resume-r')), findsNothing);
    await tapKey(t, 'approve-all-r');
    final decide = b.commands.whereType<DecideApprovals>().single;
    expect(decide.approve.single.attemptId, 'attempt-3333-uuid');
  });

  testWidgets('calls held by an unknown outcome say why and keep Stop', (
    t,
  ) async {
    final b = backendWith([
      pendingAttempt(
        'attempt-8888-uuid',
        serverId,
        'send',
        state: AttemptState.outcomeUnknown,
      ),
      attemptOne,
    ]);
    await pumpChat(t, b);
    expect(
      find.text(
        "search · Notes · Waiting — an earlier call's outcome is unknown",
      ),
      findsOneWidget,
    );
    expect(find.text('Needs approval'), findsNothing);
    expect(find.text('Waiting for your approval'), findsNothing);
    expect(
      find.text("Waiting — a tool call's outcome is unknown"),
      findsOneWidget,
    );
    expect(find.byKey(const Key('stop-r')), findsOneWidget);
  });

  testWidgets('one pending call reads as one tool', (t) async {
    final b = backendWith([attemptOne]);
    await pumpChat(t, b);
    expect(inApproval(find.text('Notes wants to run 1 tool')), findsOneWidget);
  });

  testWidgets('calls from several servers get a neutral title, no Always', (
    t,
  ) async {
    final b = backendWith([
      attemptOne,
      pendingAttempt('attempt-3333-uuid', otherServerId, 'book'),
    ]);
    await pumpChat(t, b);
    expect(inApproval(find.text('2 tool calls need your approval')), findsOne);
    expect(inApproval(find.text('book · Calendar')), findsOneWidget);
    expect(find.byKey(const Key('always-allow-r')), findsNothing);
  });

  testWidgets('local file calls never offer Always allow', (t) async {
    final b = backendWith([pendingAttempt('attempt-4444-uuid', null, 'read')]);
    await pumpChat(t, b);
    expect(inApproval(find.text('read · Local files')), findsOneWidget);
    expect(find.byKey(const Key('always-allow-r')), findsNothing);
    expect(find.byKey(const Key('approve-all-r')), findsOneWidget);
  });

  testWidgets('a server missing from the run falls back to the snapshot', (
    t,
  ) async {
    final b = backendWith(
      [pendingAttempt('attempt-5555-uuid', 'server-current-only', 'list')],
      servers: const [
        ServerConfig(
          id: 'server-current-only',
          name: 'Files',
          url: 'https://files.example.test/mcp',
          allowHttp: false,
        ),
      ],
    );
    await pumpChat(t, b);
    expect(inApproval(find.text('list · Files')), findsOneWidget);
  });

  testWidgets('Approve all sends the identical bindings in one command', (
    t,
  ) async {
    final b = backendWith([attemptOne, attemptTwo]);
    await pumpChat(t, b);
    await tapKey(t, 'approve-all-r');
    final decide = b.commands.whereType<DecideApprovals>().single;
    expect(decide.runId, 'r');
    expect(decide.deny, isEmpty);
    expect(decide.approve, hasLength(2));
    expect(identical(decide.approve[0], attemptOne.binding), isTrue);
    expect(identical(decide.approve[1], attemptTwo.binding), isTrue);
    expect(b.commands.whereType<Approve>(), isEmpty);
    expect(b.commands.whereType<ResumeRun>(), isEmpty);
  });

  testWidgets('Deny all denies every pending attempt', (t) async {
    final b = backendWith([attemptOne, attemptTwo]);
    await pumpChat(t, b);
    await tapKey(t, 'deny-all-r');
    final decide = b.commands.whereType<DecideApprovals>().single;
    expect(decide.runId, 'r');
    expect(decide.approve, isEmpty);
    expect(decide.deny, ['attempt-1111-uuid', 'attempt-2222-uuid']);
  });

  testWidgets('a per-call Deny denies only that call', (t) async {
    final b = backendWith([attemptOne, attemptTwo]);
    await pumpChat(t, b);
    await tapKey(t, 'deny-attempt-2222-uuid');
    final decide = b.commands.whereType<DecideApprovals>().single;
    expect(decide.approve, isEmpty);
    expect(decide.deny, ['attempt-2222-uuid']);
  });

  testWidgets('Always allow confirms, trusts the server, then approves all', (
    t,
  ) async {
    final b = backendWith([attemptOne, attemptTwo]);
    await pumpChat(t, b);
    await tapKey(t, 'always-allow-r');
    expect(inApproval(find.text('Always allow Notes')), findsOneWidget);
    expect(
      find.text(
        'Tools from Notes will run without asking, including tools it adds later, until you turn this off in its settings or its URL or credentials change.',
      ),
      findsOneWidget,
    );
    await t.tap(find.text('Cancel'));
    await t.pumpAndSettle();
    expect(b.commands, isEmpty);
    await tapKey(t, 'always-allow-r');
    await t.tap(find.byKey(const Key('confirm-trust')));
    await t.pumpAndSettle();
    expect(b.commands, hasLength(2));
    final trust = b.commands[0] as SetServerTrust;
    expect(trust.serverId, serverId);
    expect(trust.trusted, isTrue);
    final decide = b.commands[1] as DecideApprovals;
    expect(decide.approve, hasLength(2));
    expect(identical(decide.approve[0], attemptOne.binding), isTrue);
    expect(identical(decide.approve[1], attemptTwo.binding), isTrue);
  });

  testWidgets('a failed trust change approves nothing', (t) async {
    final b = backendWith([attemptOne])
      ..failure = const BackendFailure(FailureCode.storage);
    await pumpChat(t, b);
    await tapKey(t, 'always-allow-r');
    await t.tap(find.byKey(const Key('confirm-trust')));
    await t.pumpAndSettle();
    expect(b.commands.whereType<SetServerTrust>(), hasLength(1));
    expect(b.commands.whereType<DecideApprovals>(), isEmpty);
  });

  testWidgets('decision buttons are disabled while a command runs', (t) async {
    final b = HeldBackend();
    b.current = backendWith([attemptOne, attemptTwo]).current;
    await pumpChat(t, b);
    await t.tap(find.byKey(const Key('approve-all-r')));
    await t.pump();
    for (final key in [
      'approve-all-r',
      'deny-all-r',
      'always-allow-r',
      'deny-attempt-1111-uuid',
    ]) {
      final button = t.widget<ButtonStyleButton>(find.byKey(Key(key)));
      expect(button.onPressed, isNull, reason: key);
    }
    b.held.complete();
    await t.pumpAndSettle();
    expect(b.commands.whereType<DecideApprovals>(), hasLength(1));
  });

  testWidgets('the run row asks for approval with Stop and no Resume', (
    t,
  ) async {
    final b = backendWith([attemptOne]);
    await pumpChat(t, b);
    expect(find.text('Waiting for your approval'), findsOneWidget);
    expect(find.byKey(const Key('resume-r')), findsNothing);
    expect(find.text('Resume'), findsNothing);
    await tapKey(t, 'stop-r');
    expect(b.commands, isEmpty);
    await t.tap(find.byKey(const Key('confirm-stop')));
    await t.pumpAndSettle();
    expect(b.commands.whereType<CancelRun>().single.runId, 'r');
  });

  testWidgets('Resume appears once every call is decided', (t) async {
    final b = backendWith([
      pendingAttempt(
        'attempt-1111-uuid',
        serverId,
        'search',
        state: AttemptState.approved,
      ),
    ]);
    await pumpChat(t, b);
    expect(find.byKey(const Key('approval-r')), findsNothing);
    await tapKey(t, 'resume-r');
    expect(b.commands.whereType<ResumeRun>().single.runId, 'r');
  });

  testWidgets('a waiting run with an unresolved attempt never resumes', (
    t,
  ) async {
    final b = backendWith([
      pendingAttempt(
        'attempt-1111-uuid',
        serverId,
        'search',
        state: AttemptState.dispatching,
      ),
    ]);
    await pumpChat(t, b);
    expect(find.byKey(const Key('resume-r')), findsNothing);
    expect(find.byKey(const Key('stop-r')), findsOneWidget);
  });

  for (final unresolved in [
    AttemptState.dispatching,
    AttemptState.outcomeUnknown,
  ]) {
    testWidgets('a $unresolved attempt hides the approval card', (t) async {
      final b = backendWith([
        pendingAttempt(
          'attempt-8888-uuid',
          serverId,
          'send',
          state: unresolved,
        ),
        attemptOne,
      ]);
      await pumpChat(t, b);
      // Deciding resumes the run, which must never happen here.
      expect(find.byKey(const Key('approval-r')), findsNothing);
      expect(find.byKey(const Key('resume-r')), findsNothing);
      expect(find.byKey(const Key('stop-r')), findsOneWidget);
    });
  }

  testWidgets('a proposed call without a binding reads as Queued', (t) async {
    final b = backendWith([
      attemptOne,
      pendingAttempt('attempt-6666-uuid', serverId, 'list', bound: false),
    ]);
    await pumpChat(t, b);
    expect(inApproval(find.text('Notes wants to run 1 tool')), findsOneWidget);
    expect(find.text('list · Notes · Queued'), findsOneWidget);
    expect(find.text('search · Notes · Needs approval'), findsOneWidget);
  });

  testWidgets('transcript tool cards show names, ids only in details', (
    t,
  ) async {
    final b = backendWith([attemptOne]);
    await pumpChat(t, b);
    final title = find.text('search · Notes · Needs approval');
    expect(title, findsOneWidget);
    expect(find.textContaining(profileId), findsNothing);
    expect(find.text('Approve exact arguments'), findsNothing);
    await t.tap(title);
    await t.pumpAndSettle();
    expect(find.textContaining('Profile $profileId'), findsOneWidget);
    expect(find.textContaining('Server $serverId'), findsOneWidget);
    expect(find.textContaining('Attempt attempt-1111-uuid'), findsOneWidget);
    expect(
      find.text('Destination: https://notes.example.test/mcp'),
      findsOneWidget,
    );
    // The transcript card carries no decision buttons.
    expect(find.byKey(const Key('deny-attempt-1111-uuid')), findsOneWidget);
    expect(find.text('Approve exact arguments'), findsNothing);
  });
}
