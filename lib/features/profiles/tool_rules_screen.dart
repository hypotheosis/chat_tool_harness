import 'dart:convert';
import 'package:flutter/material.dart';
import '../../backend/backend.dart';
import '../../backend/models.dart';
import '../result_banner.dart';
import '../screen_controller.dart';

class ToolRulesScreen extends StatefulWidget {
  const ToolRulesScreen({super.key, required this.backend});
  final Backend backend;
  @override
  State<ToolRulesScreen> createState() => _ToolRulesScreenState();
}

class _ToolRulesScreenState extends State<ToolRulesScreen> {
  late final state = ScreenController(widget.backend);
  final discovered = <String, ({ToolRule rule, String configuration})>{};

  /// Tools the last discovery could not use.
  List<SkippedTool> skipped = [];
  String identity(ToolRule rule) =>
      '${rule.key.profileId}/${rule.key.serverId}/${rule.key.name}';
  List<ToolRule> get rules => {
    for (final entry in discovered.entries) entry.key: entry.value.rule,
    for (final rule in state.snapshot?.rules ?? <ToolRule>[])
      identity(rule): rule,
  }.values.toList();

  String? configurationIdentity(ToolKey key) {
    final snapshot = state.snapshot;
    if (snapshot == null) return null;
    Profile? profile;
    EndpointConfig? endpoint;
    ServerConfig? server;
    for (final p in snapshot.profiles) {
      if (p.id == key.profileId) profile = p;
    }
    if (profile == null) return null;
    for (final e in snapshot.endpoints) {
      if (e.id == profile.endpointId) endpoint = e;
    }
    if (endpoint == null) return null;
    if (key.serverId != null) {
      if (!profile.serverIds.contains(key.serverId)) return null;
      for (final s in snapshot.servers) {
        if (s.id == key.serverId) server = s;
      }
      if (server == null) return null;
    }
    // UI cache identity only, never a permission/destination digest used by the core.
    return jsonEncode([
      profile.id,
      profile.name,
      profile.endpointId,
      profile.serverIds,
      profile.instructions,
      profile.mode.name,
      profile.limits.maxInferenceRequests,
      profile.limits.maxToolAttempts,
      profile.limits.activeTimeoutMs,
      endpoint.id,
      endpoint.name,
      endpoint.baseUrl,
      endpoint.model,
      endpoint.contextTokens,
      endpoint.maxOutputTokens,
      endpoint.temperature,
      endpoint.allowHttp,
      endpoint.credential,
      if (server != null)
        [
          server.id,
          server.name,
          server.url,
          server.allowHttp,
          [
            for (final h in server.secretHeaders) [h.name, h.credential],
          ],
        ],
    ]);
  }

  /// Whether [rule]'s server is trusted, so its tools run without asking.
  bool trustedServer(ToolRule rule) =>
      state.snapshot?.servers.any(
        (s) => s.id == rule.key.serverId && s.trusted,
      ) ??
      false;

  bool hasLive(ToolRule rule) =>
      state.snapshot?.rules.any((r) => identity(r) == identity(rule)) ?? false;
  void reconcileCache() {
    discovered.removeWhere(
      (_, entry) =>
          configurationIdentity(entry.rule.key) != entry.configuration ||
          hasLive(entry.rule),
    );
  }

  void remember(List<ToolRule> observed, String? configuration) {
    for (final rule in observed) {
      final key = identity(rule);
      if (configuration == null ||
          configurationIdentity(rule.key) != configuration ||
          hasLive(rule)) {
        discovered.remove(key);
      } else {
        discovered[key] = (rule: rule, configuration: configuration);
      }
    }
  }

  Future<void> discover(String profile, String server) async {
    final configuration = configurationIdentity(
      ToolKey(profileId: profile, serverId: server, name: ''),
    );
    final reply = await state.perform(DiscoverServer(profile, server));
    if (reply != null && mounted) {
      setState(() {
        discovered.removeWhere(
          (_, entry) =>
              entry.rule.key.profileId == profile &&
              entry.rule.key.serverId == server,
        );
        remember(reply.discoveredRules, configuration);
      });
      setState(() {
        skipped = reply.skippedTools;
      });
      state.succeed(
        discoverySummary(
          reply.discoveredRules.length,
          reply.skippedTools.length,
        ),
      );
    }
  }

  Future<void> change(ToolRule rule, PermissionRule value) async {
    final configuration = configurationIdentity(rule.key);
    final reply = await state.perform(
      SetRule(rule.key, rule.schemaDigest, rule.destinationDigest, value),
    );
    if (reply != null && mounted) {
      setState(() {
        remember(reply.discoveredRules, configuration);
      });
    }
  }

  @override
  void initState() {
    super.initState();
    state.addListener(reconcileCache);
    state.connect();
  }

  @override
  void dispose() {
    state.removeListener(reconcileCache);
    state.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: state,
    builder: (context, _) => Scaffold(
      appBar: AppBar(
        title: const Text('Tool permissions'),
        bottom: settingsProgress(state),
      ),
      body: SafeArea(
        top: false,
        child: ListView(
          padding: const EdgeInsets.all(16),
          children: [
            const Text(
              'New or changed tools default to Ask. Allow permits automatic writes as well as reads. Rules are scoped to exact profile / server / tool; schema or destination changes revoke trust.',
            ),
            const Text(
              'MCP rule changes perform safe re-discovery and reject changed schemas/destinations. No business tool is called. Diagnostic observations are not permanent availability guarantees.',
            ),
            for (final p in state.snapshot?.profiles ?? <Profile>[])
              for (final s in p.serverIds)
                OutlinedButton(
                  onPressed: state.loading ? null : () => discover(p.id, s),
                  child: Text('Discover rules ${p.name} • ${p.id} / $s'),
                ),
            ResultBanner(state: state),
            SkippedToolList(skipped),
            if (rules.isEmpty && !state.loading)
              const Text(
                'No observed tools. Discover tools from a linked server or start an Agent conversation first.',
              ),
            for (final rule in rules)
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Text(
                        '${rule.key.profileId} / ${rule.key.serverId ?? 'Local files'} / ${rule.key.name}',
                      ),
                      Text(
                        hasLive(rule)
                            ? 'Current core observation / effective policy'
                            : 'Cached diagnostic observation — not live permission. Applying a change requires fresh core validation/discovery.',
                      ),
                      // Block rules still apply to a trusted server.
                      if (trustedServer(rule))
                        const Text(
                          'Trusted server: all tools run without asking',
                        ),
                      ExpansionTile(
                        title: const Text('Observed schema'),
                        children: [SelectableText(rule.schemaJson)],
                      ),
                      DropdownButton<PermissionRule>(
                        value: rule.rule,
                        isExpanded: true,
                        items: [
                          for (final r in PermissionRule.values)
                            DropdownMenuItem(
                              value: r,
                              child: Text(switch (r) {
                                PermissionRule.ask => 'Ask',
                                PermissionRule.allow => 'Allow',
                                PermissionRule.block => 'Block',
                              }),
                            ),
                        ],
                        onChanged: state.loading
                            ? null
                            : (value) {
                                if (value != null) {
                                  change(rule, value);
                                }
                              },
                      ),
                    ],
                  ),
                ),
              ),
          ],
        ),
      ),
    ),
  );
}
