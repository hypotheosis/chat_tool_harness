import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/chat/chat_screen.dart';
import 'package:chat_tool_harness/features/profiles/profile_screen.dart';
import 'package:chat_tool_harness/features/profiles/tool_rules_screen.dart';
import '../support/ui_fixture.dart';

void main() {
  testWidgets('rules_can_be_discovered_without_a_live_run', (t) async {
    final b = UiBackend();
    b.reply = CommandReply(
      discoveredRules: [
        ToolRule(
          key: binding.key,
          schemaJson: '{}',
          schemaDigest: 'schema',
          destinationDigest: 'destination',
          rule: PermissionRule.ask,
        ),
      ],
    );
    await t.pumpWidget(MaterialApp(home: ToolRulesScreen(backend: b)));
    await t.pumpAndSettle();
    await t.tap(find.text('Discover rules Profile • p / s'));
    await t.pumpAndSettle();
    expect(b.commands.whereType<DiscoverServer>().single.profileId, 'p');
    expect(find.text('Ask'), findsOneWidget);
    await t.tap(find.text('Ask'));
    await t.pumpAndSettle();
    await t.tap(find.text('Block').last);
    await t.pumpAndSettle();
    expect(b.commands.whereType<SetRule>().single.rule, PermissionRule.block);
  });
  testWidgets('chat_follows_profile_without_explicit_apply', (t) async {
    final b = UiBackend();
    await t.pumpWidget(
      MaterialApp(
        home: ChatScreen(backend: b, conversationId: 'c'),
      ),
    );
    await t.pumpAndSettle();
    expect(find.text('Old profile'), findsOneWidget);
    expect(find.text('Apply current profile'), findsNothing);
    await t.tap(find.byTooltip('Profile settings'));
    await t.pumpAndSettle();
    expect(find.byType(ProfileScreen), findsOneWidget);
    expect(b.commands.whereType<ApplyProfile>(), isEmpty);
    await t.pumpWidget(const SizedBox());
    await t.pumpAndSettle();
    expect(b.closed, isFalse);
    expect(b.commands.whereType<CancelRun>(), isEmpty);
  });
  testWidgets('rule_editor_defaults_ask', (t) async {
    final b = UiBackend();
    b.current = AppSnapshot(
      rules: [
        ToolRule(
          key: binding.key,
          schemaJson: '{}',
          schemaDigest: 'schema',
          destinationDigest: 'destination',
          rule: PermissionRule.ask,
        ),
      ],
    );
    await t.pumpWidget(MaterialApp(home: ToolRulesScreen(backend: b)));
    await t.pumpAndSettle();
    expect(find.text('Ask'), findsOneWidget);
    expect(find.textContaining('automatic writes'), findsOneWidget);
    await t.tap(find.text('Ask'));
    await t.pumpAndSettle();
    await t.tap(find.text('Allow').last);
    await t.pumpAndSettle();
    expect(b.commands.whereType<SetRule>().single.rule, PermissionRule.allow);
    expect(
      b.commands.whereType<SetRule>().single.destinationDigest,
      'destination',
    );
  });
  testWidgets('duplicate_names_select_by_id_and_explicit_chat_only', (t) async {
    final b = UiBackend();
    b.current = const AppSnapshot(
      endpoints: [
        endpoint,
        EndpointConfig(
          id: 'other',
          name: 'Endpoint',
          baseUrl: 'https://other.test',
          model: 'm',
          contextTokens: 4000,
          maxOutputTokens: 50,
          temperature: 0,
          allowHttp: false,
        ),
      ],
      profiles: [profile],
      servers: [
        ServerConfig(
          id: 's',
          name: 'Server',
          url: 'https://one.test',
          allowHttp: false,
        ),
        ServerConfig(
          id: 'other-server',
          name: 'Server',
          url: 'https://two.test',
          allowHttp: false,
        ),
      ],
    );
    await t.pumpWidget(
      MaterialApp(
        home: ProfileScreen(backend: b, profile: profile),
      ),
    );
    await t.pumpAndSettle();
    await t.tap(find.byType(DropdownButtonFormField<String>));
    await t.pumpAndSettle();
    await t.tap(find.text('Endpoint • other').last);
    await t.pumpAndSettle();
    await t.scrollUntilVisible(
      find.text('Server • other-server'),
      150,
      scrollable: find.byType(Scrollable).first,
    );
    await t.tap(find.text('Server • other-server'));
    await t.pumpAndSettle();
    await t.scrollUntilVisible(
      find.text('Chat only (no MCP or local tools)'),
      -150,
      scrollable: find.byType(Scrollable).first,
    );
    await t.tap(find.text('Chat only (no MCP or local tools)'));
    await t.pumpAndSettle();
    await t.scrollUntilVisible(
      find.text('Save profile'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.tap(find.text('Save profile'));
    await t.pumpAndSettle();
    expect(
      b.commands.whereType<SaveProfile>().single.profile.mode,
      ProfileMode.chatOnly,
    );
    expect(
      b.commands.whereType<SaveProfile>().single.profile.endpointId,
      'other',
    );
    expect(
      b.commands.whereType<SaveProfile>().single.profile.serverIds,
      containsAll(['s', 'other-server']),
    );
  });
  testWidgets('failed profile save shows the failure under the button', (
    t,
  ) async {
    final b = UiBackend();
    await t.pumpWidget(
      MaterialApp(
        home: ProfileScreen(backend: b, profile: profile),
      ),
    );
    await t.pumpAndSettle();
    b.failure = const BackendFailure(
      FailureCode.configuration,
      detail: FailureDetail(
        targetKind: 'profile',
        targetName: 'Profile',
        step: 'request',
        reason: 'endpoint missing',
      ),
    );
    await t.scrollUntilVisible(
      find.text('Save profile'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.pumpAndSettle();
    await t.tap(find.text('Save profile'));
    await t.pumpAndSettle();
    expect(find.text('Failed'), findsOneWidget);
    expect(find.textContaining('Profile "Profile"'), findsOneWidget);
    expect(find.textContaining('endpoint missing'), findsOneWidget);
    expect(find.byType(ProfileScreen), findsOneWidget);
  });
  testWidgets('failed rule discovery shows the server failure', (t) async {
    final b = UiBackend()
      ..failure = const BackendFailure(
        FailureCode.unavailableTransport,
        detail: FailureDetail(
          targetKind: 'mcpServer',
          targetName: 'Server',
          step: 'listTools',
          reason: 'connection refused',
        ),
      );
    await t.pumpWidget(MaterialApp(home: ToolRulesScreen(backend: b)));
    await t.pumpAndSettle();
    await t.tap(find.text('Discover rules Profile • p / s'));
    await t.pumpAndSettle();
    expect(find.text('Failed'), findsOneWidget);
    expect(
      find.textContaining('MCP server "Server": failed during tool listing'),
      findsOneWidget,
    );
  });
}
