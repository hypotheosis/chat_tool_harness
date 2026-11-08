import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/settings/server_screen.dart';
import '../support/ui_fixture.dart';

void main() {
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
        find.textContaining('No business tool was invoked'),
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
