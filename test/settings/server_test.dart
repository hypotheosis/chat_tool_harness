import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/settings/server_screen.dart';
import '../support/ui_fixture.dart';

final testButton = find.text('Test connection and discover tools');

/// Pumps the server screen for the fixture server with profile `p` selected.
Future<void> pumpServerScreen(WidgetTester t, UiBackend b) async {
  await t.pumpWidget(
    MaterialApp(
      home: ServerScreen(backend: b, server: b.current.servers.single),
    ),
  );
  await t.pumpAndSettle();
  await t.scrollUntilVisible(
    testButton,
    200,
    scrollable: find.byType(Scrollable).first,
  );
  await t.tap(find.byType(DropdownButtonFormField<String>));
  await t.pumpAndSettle();
  await t.tap(find.text('Profile • p').last);
  await t.pumpAndSettle();
}

void main() {
  testWidgets('failed connection test shows server, status and reason', (
    t,
  ) async {
    final b = UiBackend();
    await pumpServerScreen(t, b);
    b.failure = const BackendFailure(
      FailureCode.authentication,
      detail: FailureDetail(
        targetKind: 'mcpServer',
        targetName: 'tools',
        step: 'initialize',
        url: 'https://example.com/mcp',
        status: 401,
        reason: 'unknown API key',
      ),
    );
    await t.tap(testButton);
    await t.pumpAndSettle();
    expect(find.textContaining('MCP server "tools"'), findsOneWidget);
    expect(find.textContaining('HTTP 401'), findsOneWidget);
    expect(find.textContaining('unknown API key'), findsOneWidget);
    expect(find.textContaining('Task 12'), findsNothing);
    expect(find.textContaining('https://example.com/mcp'), findsNothing);
    await t.scrollUntilVisible(
      find.text('Details'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.tap(find.text('Details'));
    await t.pumpAndSettle();
    expect(find.textContaining('URL: https://example.com/mcp'), findsOneWidget);
    expect(find.textContaining('Step: initialization'), findsOneWidget);
    expect(find.textContaining('Target: MCP server "tools"'), findsOneWidget);
    expect(find.textContaining('mcpServer'), findsNothing);
    expect(find.text('Reload'), findsNothing);
    expect(find.text('Hide details'), findsOneWidget);
  });

  testWidgets('only a failed load offers Reload', (t) async {
    final b = UiBackend()
      ..snapshotResponses.add(
        // Fails only once awaited, so the error is never unhandled.
        Future(() => throw const BackendFailure(FailureCode.storage)),
      );
    await t.pumpWidget(
      MaterialApp(
        home: ServerScreen(backend: b, server: b.current.servers.single),
      ),
    );
    await t.pumpAndSettle();
    await t.scrollUntilVisible(
      find.text('Reload'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.tap(find.text('Reload'));
    await t.pumpAndSettle();
    expect(find.text('Failed'), findsNothing);
    expect(b.snapshotCalls, 2);
    await t.scrollUntilVisible(
      testButton,
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.tap(testButton);
    await t.pumpAndSettle();
    expect(find.text('Failed'), findsOneWidget);
    expect(find.text('Reload'), findsNothing);
  });

  testWidgets('successful connection test shows tool count', (t) async {
    final b = UiBackend()
      ..reply = const CommandReply(discoveredTools: ['a', 'b']);
    await pumpServerScreen(t, b);
    await t.tap(testButton);
    await t.pumpAndSettle();
    expect(
      find.textContaining('Connected to Server: 2 tools available'),
      findsOneWidget,
    );
    expect(find.text('Failed'), findsNothing);
  });

  testWidgets('skipped tools are counted and listed with their reason', (
    t,
  ) async {
    final b = UiBackend()
      ..reply = const CommandReply(
        discoveredTools: ['a'],
        skippedTools: [SkippedTool('remote', 'refers to another document')],
      );
    await pumpServerScreen(t, b);
    await t.tap(testButton);
    await t.pumpAndSettle();
    expect(
      find.textContaining(
        'Connected to Server: 1 tool available; 1 skipped (see below)',
      ),
      findsOneWidget,
    );
    await t.scrollUntilVisible(
      find.text('refers to another document'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    expect(find.text('remote'), findsOneWidget);
  });

  testWidgets('a new test clears the previous failure', (t) async {
    final b = UiBackend()
      ..failure = const BackendFailure(FailureCode.unavailableTransport);
    await pumpServerScreen(t, b);
    await t.tap(testButton);
    await t.pumpAndSettle();
    expect(find.text('Failed'), findsOneWidget);
    b.failure = null;
    b.reply = const CommandReply(discoveredTools: ['a']);
    await t.tap(testButton);
    await t.pumpAndSettle();
    expect(find.text('Failed'), findsNothing);
    expect(find.textContaining('1 tool available'), findsOneWidget);
  });

  testWidgets(
    'server_diagnostic_requires_linked_profile_and_never_calls_tools',
    (t) async {
      final b = UiBackend();
      final server = b.current.servers.single;
      await t.pumpWidget(
        MaterialApp(
          home: ServerScreen(backend: b, server: server),
        ),
      );
      await t.pumpAndSettle();
      final test = find.text('Test connection and discover tools');
      await t.scrollUntilVisible(
        test,
        200,
        scrollable: find.byType(Scrollable).first,
      );
      await t.tap(test);
      await t.pumpAndSettle();
      expect(b.commands.whereType<DiscoverServer>(), isEmpty);
      expect(find.textContaining('select a saved profile'), findsOneWidget);
      await t.tap(find.byType(DropdownButtonFormField<String>));
      await t.pumpAndSettle();
      await t.tap(find.text('Profile • p').last);
      await t.pumpAndSettle();
      b.reply = const CommandReply(discoveredTools: ['write']);
      await t.tap(test);
      await t.pumpAndSettle();
      expect(b.commands.whereType<DiscoverServer>().single.serverId, 's');
      expect(b.commands.whereType<DiscoverServer>().single.profileId, 'p');
      expect(
        find.textContaining('Connected to Server: 1 tool available'),
        findsOneWidget,
      );
      expect(b.commands.whereType<SaveProfile>(), isEmpty);
    },
  );
  testWidgets('server_configuration_preserves_reference_headers', (t) async {
    final b = UiBackend();
    const server = ServerConfig(
      id: 's',
      name: 'Server',
      url: 'https://example.test/mcp',
      allowHttp: false,
      secretHeaders: [
        SecretHeader(name: 'Authorization', credential: 'reference-only'),
      ],
    );
    await t.pumpWidget(
      MaterialApp(
        home: ServerScreen(backend: b, server: server),
      ),
    );
    await t.pumpAndSettle();
    await t.scrollUntilVisible(
      find.text('Save server'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.tap(find.text('Save server'));
    await t.pumpAndSettle();
    expect(
      b.commands
          .whereType<SaveServer>()
          .single
          .server
          .secretHeaders
          .single
          .credential,
      'reference-only',
    );
  });
}
