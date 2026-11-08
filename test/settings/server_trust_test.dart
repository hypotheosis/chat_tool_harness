import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/profiles/tool_rules_screen.dart';
import 'package:chat_tool_harness/features/settings/server_screen.dart';
import '../support/ui_fixture.dart';

ServerConfig server({bool trusted = false}) => ServerConfig(
  id: 's',
  name: 'Server',
  url: 'https://example.test/mcp',
  allowHttp: false,
  trusted: trusted,
);

/// Applies trust changes to its snapshot, as the real backend does.
class TrustBackend extends UiBackend {
  TrustBackend({bool trusted = false}) {
    setServer(trusted);
  }
  Completer<void>? hold;

  void setServer(bool trusted) => current = AppSnapshot(
    sequence: current.sequence,
    endpoints: current.endpoints,
    servers: [server(trusted: trusted)],
    profiles: current.profiles,
    conversations: current.conversations,
    runs: current.runs,
    rules: current.rules,
  );

  @override
  Future<CommandReply> execute(AppCommand command) async {
    if (hold case final h?) await h.future;
    if (command is SetServerTrust && failure == null) {
      setServer(command.trusted);
    }
    return super.execute(command);
  }
}

final trustSwitch = find.byKey(const Key('trust-server'));

Future<void> pumpServer(WidgetTester t, UiBackend b, ServerConfig? s) async {
  await t.pumpWidget(
    MaterialApp(
      home: ServerScreen(backend: b, server: s),
    ),
  );
  await t.pumpAndSettle();
  await t.scrollUntilVisible(
    trustSwitch,
    200,
    scrollable: find.byType(Scrollable).first,
  );
  await t.pumpAndSettle();
}

bool switchValue(WidgetTester t) => t.widget<SwitchListTile>(trustSwitch).value;

void main() {
  testWidgets('the trust switch explains trust and reflects the server', (
    t,
  ) async {
    final b = TrustBackend(trusted: true);
    await pumpServer(t, b, server(trusted: true));
    expect(find.text('Trust this server'), findsOneWidget);
    expect(find.text(trustExplanation('Server')), findsOneWidget);
    expect(switchValue(t), isTrue);
  });

  testWidgets('turning trust on sends SetServerTrust and shows the result', (
    t,
  ) async {
    final b = TrustBackend();
    await pumpServer(t, b, server());
    expect(switchValue(t), isFalse);
    await t.tap(trustSwitch);
    await t.pumpAndSettle();
    final command = b.commands.whereType<SetServerTrust>().single;
    expect(command.serverId, 's');
    expect(command.trusted, isTrue);
    expect(switchValue(t), isTrue);
    expect(find.text('Server trusted.'), findsOneWidget);
    await t.tap(trustSwitch);
    await t.pumpAndSettle();
    expect(b.commands.whereType<SetServerTrust>().last.trusted, isFalse);
    expect(switchValue(t), isFalse);
    expect(find.text('Server no longer trusted.'), findsOneWidget);
  });

  testWidgets('a failed trust change shows the failure and keeps the value', (
    t,
  ) async {
    final b = TrustBackend()
      ..failure = const BackendFailure(FailureCode.storage);
    await pumpServer(t, b, server());
    await t.tap(trustSwitch);
    await t.pumpAndSettle();
    expect(find.text('Failed'), findsOneWidget);
    expect(switchValue(t), isFalse);
  });

  testWidgets('the trust switch is disabled while a command runs', (t) async {
    final b = TrustBackend();
    await pumpServer(t, b, server());
    b.hold = Completer<void>();
    await t.tap(trustSwitch);
    await t.pump();
    expect(t.widget<SwitchListTile>(trustSwitch).onChanged, isNull);
    b.hold!.complete();
    await t.pumpAndSettle();
    expect(t.widget<SwitchListTile>(trustSwitch).onChanged, isNotNull);
  });

  testWidgets('an unsaved server cannot be trusted', (t) async {
    final b = TrustBackend();
    await pumpServer(t, b, null);
    expect(t.widget<SwitchListTile>(trustSwitch).onChanged, isNull);
    expect(switchValue(t), isFalse);
  });

  testWidgets('tool rules note a trusted server and keep rules editable', (
    t,
  ) async {
    const rule = ToolRule(
      key: ToolKey(profileId: 'p', serverId: 's', name: 'write'),
      schemaJson: '{}',
      schemaDigest: 'schema',
      destinationDigest: 'destination',
      rule: PermissionRule.block,
    );
    for (final trusted in [true, false]) {
      final b = UiBackend()
        ..current = AppSnapshot(
          endpoints: [endpoint],
          profiles: [profile],
          servers: [server(trusted: trusted)],
          rules: const [rule],
        );
      await t.pumpWidget(
        MaterialApp(
          key: UniqueKey(),
          home: ToolRulesScreen(backend: b),
        ),
      );
      await t.pumpAndSettle();
      expect(
        find.text('Trusted server: all tools run without asking'),
        trusted ? findsOneWidget : findsNothing,
      );
      final dropdown = t.widget<DropdownButton<PermissionRule>>(
        find.byType(DropdownButton<PermissionRule>),
      );
      expect(dropdown.onChanged, isNotNull);
    }
  });
}
