import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/settings/endpoint_screen.dart';
import 'package:chat_tool_harness/features/settings/last_connection_test.dart';
import '../support/ui_fixture.dart';
import 'server_test.dart' show pumpServerScreen, testButton;

final at = DateTime.utc(2026, 11, 10, 14, 3);
const authFailure = BackendFailure(
  FailureCode.authentication,
  detail: FailureDetail(
    targetKind: 'mcpServer',
    targetName: 'Server',
    step: 'initialize',
    status: 401,
    reason: 'unknown API key',
  ),
);

/// A [UiBackend] whose snapshot holds [tests]; a server test stores
/// [nextServerTest] and then fails with it.
class StoringBackend extends UiBackend {
  StoringBackend(List<ConnectionTest> tests) {
    withTests(tests);
  }
  ConnectionTest? nextServerTest;
  void withTests(List<ConnectionTest> tests) => current = AppSnapshot(
    sequence: current.sequence,
    endpoints: current.endpoints,
    servers: current.servers,
    profiles: current.profiles,
    conversations: current.conversations,
    runs: current.runs,
    rules: current.rules,
    connectionTests: tests,
  );
  @override
  Future<CommandReply> execute(AppCommand command) async {
    final next = nextServerTest;
    if (command is DiscoverServer && next != null) {
      commands.add(command);
      withTests([next]);
      throw next.failure!;
    }
    return super.execute(command);
  }
}

Future<void> scrollTo(WidgetTester t, Finder finder) => t.scrollUntilVisible(
  finder,
  200,
  scrollable: find.byType(Scrollable).first,
);

void main() {
  test('tested-at label is local date and minute', () {
    expect(testedAtLabel(DateTime(2026, 1, 2, 9, 5, 59)), '2026-01-02 09:05');
  });

  testWidgets('endpoint screen shows the stored result of each test kind', (
    t,
  ) async {
    final b = StoringBackend([
      ConnectionTest(
        kind: ConnectionTestKind.discoverModels,
        endpointId: 'e',
        testedAt: at,
        modelCount: 3,
      ),
      ConnectionTest(
        kind: ConnectionTestKind.probeTools,
        endpointId: 'e',
        testedAt: at,
        failure: const BackendFailure(FailureCode.unavailableTransport),
      ),
      ConnectionTest(
        kind: ConnectionTestKind.discoverModels,
        endpointId: 'other',
        testedAt: at,
        modelCount: 9,
      ),
    ]);
    await t.pumpWidget(
      MaterialApp(
        home: EndpointScreen(backend: b, endpoint: endpoint),
      ),
    );
    await t.pumpAndSettle();
    final label = testedAtLabel(at);
    await scrollTo(t, find.text('Model discovery last tested $label'));
    expect(find.text('3 models available'), findsOneWidget);
    expect(find.text('9 models available'), findsNothing);
    await scrollTo(
      t,
      find.text('Tool support probe last tested $label: failed'),
    );
    expect(find.textContaining('could not be reached'), findsOneWidget);
    // Nothing was run: the results come from the backend's snapshot.
    expect(b.commands, isEmpty);
  });

  testWidgets('server screen shows the stored result and skipped tools', (
    t,
  ) async {
    final b = StoringBackend([
      ConnectionTest(
        kind: ConnectionTestKind.discoverServer,
        serverId: 's',
        profileId: 'p',
        testedAt: at,
        toolCount: 2,
        skippedTools: const [SkippedTool('bad', 'schema is not an object')],
      ),
    ]);
    await pumpServerScreen(t, b);
    await scrollTo(t, find.text('Connection last tested ${testedAtLabel(at)}'));
    expect(
      find.text('2 tools available; 1 skipped (see below)'),
      findsOneWidget,
    );
    await scrollTo(t, find.text('schema is not an object'));
    expect(find.text('bad'), findsOneWidget);
  });

  testWidgets('a failed server test replaces the stored result on screen', (
    t,
  ) async {
    final b = StoringBackend([
      ConnectionTest(
        kind: ConnectionTestKind.discoverServer,
        serverId: 's',
        profileId: 'p',
        testedAt: at,
        toolCount: 2,
        skippedTools: const [SkippedTool('bad', 'schema is not an object')],
      ),
    ]);
    final later = at.add(const Duration(hours: 1));
    b.nextServerTest = ConnectionTest(
      kind: ConnectionTestKind.discoverServer,
      serverId: 's',
      profileId: 'p',
      testedAt: later,
      failure: authFailure,
    );
    await pumpServerScreen(t, b);
    await t.tap(testButton);
    await t.pumpAndSettle();
    await scrollTo(
      t,
      find.text('Connection last tested ${testedAtLabel(later)}: failed'),
    );
    // The live banner and the stored result both name the failure.
    expect(find.textContaining('unknown API key'), findsNWidgets(2));
    expect(
      find.text('Connection last tested ${testedAtLabel(at)}'),
      findsNothing,
    );
    expect(find.text('bad'), findsNothing);
  });

  testWidgets('a never-tested server shows no stored result', (t) async {
    final b = StoringBackend(const []);
    await pumpServerScreen(t, b);
    expect(find.textContaining('last tested'), findsNothing);
  });
}
