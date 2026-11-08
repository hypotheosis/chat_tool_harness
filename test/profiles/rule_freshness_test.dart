import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/profiles/tool_rules_screen.dart';
import '../support/ui_fixture.dart';

const cached = ToolRule(
  key: ToolKey(profileId: 'p', serverId: 's', name: 'write'),
  schemaJson: '{"old":true}',
  schemaDigest: 'old-schema',
  destinationDigest: 'old-destination',
  rule: PermissionRule.allow,
);
AppSnapshot configured({
  List<ToolRule> rules = const [],
  List<Profile> profiles = const [profile],
  ServerConfig server = const ServerConfig(
    id: 's',
    name: 'Server',
    url: 'https://example.test/mcp',
    allowHttp: false,
  ),
}) => AppSnapshot(
  endpoints: [endpoint],
  profiles: profiles,
  servers: [server],
  rules: rules,
);
Future<UiBackend> discover(WidgetTester t) async {
  final b = UiBackend()
    ..current = configured()
    ..reply = const CommandReply(discoveredRules: [cached]);
  await t.pumpWidget(MaterialApp(home: ToolRulesScreen(backend: b)));
  await t.pumpAndSettle();
  await t.tap(find.text('Discover rules Profile • p / s'));
  await t.pumpAndSettle();
  expect(
    t
        .widget<DropdownButton<PermissionRule>>(
          find.byType(DropdownButton<PermissionRule>),
        )
        .value,
    PermissionRule.allow,
  );
  return b;
}

void publish(UiBackend b, AppSnapshot snapshot) {
  final sequence = b.current.sequence + 1;
  b.current = snapshot;
  b.emit(AppEvent(sequence: sequence, conversationId: 'c'));
}

void main() {
  for (final current in [PermissionRule.ask, PermissionRule.block]) {
    testWidgets(
      'R4 current $current and new schema override cached Allow including command digests',
      (t) async {
        final b = await discover(t);
        final fresh = ToolRule(
          key: const ToolKey(profileId: 'p', serverId: 's', name: 'write'),
          schemaJson: '{"fresh":true}',
          schemaDigest: 'fresh-schema',
          destinationDigest: 'fresh-destination',
          rule: current,
        );
        publish(b, configured(rules: [fresh]));
        await t.pumpAndSettle();
        expect(
          t
              .widget<DropdownButton<PermissionRule>>(
                find.byType(DropdownButton<PermissionRule>),
              )
              .value,
          current,
        );
        await t.tap(find.text('Observed schema'));
        await t.pumpAndSettle();
        expect(find.text(fresh.schemaJson), findsOneWidget);
        expect(find.text(cached.schemaJson), findsNothing);
        final choice = current == PermissionRule.ask
            ? PermissionRule.block
            : PermissionRule.ask;
        await t.tap(find.byType(DropdownButton<PermissionRule>));
        await t.pumpAndSettle();
        await t.tap(
          find.text(choice == PermissionRule.ask ? 'Ask' : 'Block').last,
        );
        await t.pumpAndSettle();
        final command = b.commands.whereType<SetRule>().single;
        expect(command.schemaDigest, fresh.schemaDigest);
        expect(command.destinationDigest, fresh.destinationDigest);
        expect(command.rule, choice);
        expect(
          t
              .widget<DropdownButton<PermissionRule>>(
                find.byType(DropdownButton<PermissionRule>),
              )
              .value,
          current,
        );
      },
    );
  }
  testWidgets(
    'R4 live snapshots retire cached policy and never resurrect it after observations expire',
    (t) async {
      final b = await discover(t);
      const fresh = ToolRule(
        key: ToolKey(profileId: 'p', serverId: 's', name: 'write'),
        schemaJson: '{}',
        schemaDigest: 'fresh',
        destinationDigest: 'current',
        rule: PermissionRule.block,
      );
      publish(b, configured(rules: [fresh]));
      await t.pumpAndSettle();
      expect(
        t
            .widget<DropdownButton<PermissionRule>>(
              find.byType(DropdownButton<PermissionRule>),
            )
            .value,
        PermissionRule.block,
      );
      publish(b, configured());
      await t.pumpAndSettle();
      expect(find.byType(DropdownButton<PermissionRule>), findsNothing);
    },
  );
  final edits = <String, AppSnapshot>{
    'profile removal': configured(profiles: []),
    'server route': configured(
      server: const ServerConfig(
        id: 's',
        name: 'Server',
        url: 'https://changed.test/mcp',
        allowHttp: false,
      ),
    ),
    'server header reference': configured(
      server: const ServerConfig(
        id: 's',
        name: 'Server',
        url: 'https://example.test/mcp',
        allowHttp: false,
        secretHeaders: [
          SecretHeader(name: 'Authorization', credential: 'new-ref'),
        ],
      ),
    ),
    'profile unlink': configured(
      profiles: [
        const Profile(
          id: 'p',
          name: 'Profile',
          endpointId: 'e',
          serverIds: [],
          instructions: '',
          limits: RunLimits(),
        ),
      ],
    ),
  };
  for (final edit in edits.entries) {
    testWidgets('R4 invalidate cached diagnostics after ${edit.key}', (
      t,
    ) async {
      final b = await discover(t);
      publish(b, edit.value);
      await t.pumpAndSettle();
      expect(find.byType(DropdownButton<PermissionRule>), findsNothing);
      expect(b.commands.whereType<DiscoverServer>(), hasLength(1));
    });
  }
  testWidgets(
    'R4 diagnostic fallback is visibly cached and not current live permission',
    (t) async {
      await discover(t);
      expect(
        find.textContaining(
          'Cached diagnostic observation — not live permission',
        ),
        findsOneWidget,
      );
    },
  );
}
