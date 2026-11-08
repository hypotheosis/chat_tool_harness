import 'dart:async';
import 'package:flutter/material.dart';
import 'backend/backend.dart';
import 'backend/engine_lifecycle.dart';
import 'backend/models.dart';
import 'features/screen_controller.dart';
import 'features/chat/chat_screen.dart';
import 'features/settings/endpoint_screen.dart';
import 'features/settings/server_screen.dart';
import 'features/profiles/profile_screen.dart';
import 'features/profiles/tool_rules_screen.dart';
import 'features/files/file_controller.dart';
import 'features/history/history_screen.dart';

class AgentApp extends StatelessWidget {
  const AgentApp({
    super.key,
    required this.backend,
    this.documents,
    this.lifecycle,
  });
  final EngineLifecycle? lifecycle;
  final Backend backend;
  final AndroidDocumentAccess? documents;
  @override
  Widget build(BuildContext context) => MaterialApp(
    title: 'Agent chat',
    theme: ThemeData(colorSchemeSeed: Colors.indigo, useMaterial3: true),
    home: _Home(backend: backend, documents: documents, lifecycle: lifecycle),
  );
}

class _Home extends StatefulWidget {
  const _Home({required this.backend, this.documents, this.lifecycle});
  final EngineLifecycle? lifecycle;
  final Backend backend;
  final AndroidDocumentAccess? documents;
  @override
  State<_Home> createState() => _HomeState();
}

class _HomeState extends State<_Home> {
  late final state = ScreenController(widget.backend);
  @override
  void initState() {
    super.initState();
    state.connect();
    widget.lifecycle?.openConversation.addListener(routeNotification);
    WidgetsBinding.instance.addPostFrameCallback((_) => routeNotification());
  }

  void routeNotification() {
    final conversation = widget.lifecycle?.openConversation.value;
    if (conversation == null || !mounted) return;
    widget.lifecycle!.openConversation.value = null;
    unawaited(
      open(
        ChatScreen(
          backend: widget.backend,
          conversationId: conversation,
          documents: widget.documents,
        ),
      ),
    );
  }

  @override
  void dispose() {
    widget.lifecycle?.openConversation.removeListener(routeNotification);
    state.dispose();
    super.dispose();
  }

  Future<void> open(Widget screen) async {
    await Navigator.push(
      context,
      MaterialPageRoute<void>(builder: (_) => screen),
    );
    if (mounted) await state.refresh();
  }

  Future<void> create() async {
    final profiles = state.snapshot?.profiles ?? [];
    if (profiles.isEmpty) {
      await open(_Settings(backend: widget.backend));
      return;
    }
    final id = await showDialog<String>(
      context: context,
      builder: (context) => SimpleDialog(
        title: const Text('New conversation with profile'),
        children: [
          for (final p in profiles)
            SimpleDialogOption(
              onPressed: () => Navigator.pop(context, p.id),
              child: Text('${p.name} • ${p.id}'),
            ),
        ],
      ),
    );
    if (id == null) return;
    final reply = await state.perform(CreateConversation(id));
    if (mounted && reply?.id != null) {
      await open(
        ChatScreen(
          backend: widget.backend,
          conversationId: reply!.id!,
          documents: widget.documents,
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: state,
    builder: (context, _) => Scaffold(
      appBar: AppBar(
        title: const Text('Conversations'),
        actions: [
          IconButton(
            tooltip: 'History',
            icon: const Icon(Icons.history),
            onPressed: () => open(
              HistoryScreen(
                backend: widget.backend,
                documents: widget.documents,
              ),
            ),
          ),
          IconButton(
            onPressed: () => open(_Settings(backend: widget.backend)),
            icon: const Icon(Icons.settings),
            tooltip: 'Settings',
          ),
        ],
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          ScreenStatus(state),
          if (widget.lifecycle != null)
            ValueListenableBuilder<String?>(
              valueListenable: widget.lifecycle!.notice,
              builder: (_, message, _) =>
                  message == null ? const SizedBox.shrink() : Text(message),
            ),
          FilledButton(
            onPressed: state.loading ? null : create,
            child: const Text('New conversation'),
          ),
          if (state.snapshot?.conversations.isEmpty ?? false)
            const Text(
              'No conversations. Configure an endpoint and profile in Settings, then start a conversation.',
            ),
          for (final c in state.snapshot?.conversations ?? <Conversation>[])
            ListTile(
              title: Text(c.profile.name),
              subtitle: Text('${c.id}\n${c.profile.mode.name}'),
              isThreeLine: true,
              onTap: () => open(
                ChatScreen(
                  backend: widget.backend,
                  conversationId: c.id,
                  documents: widget.documents,
                ),
              ),
            ),
        ],
      ),
    ),
  );
}

class _Settings extends StatefulWidget {
  const _Settings({required this.backend});
  final Backend backend;
  @override
  State<_Settings> createState() => _SettingsState();
}

class _SettingsState extends State<_Settings> {
  late final state = ScreenController(widget.backend);
  @override
  void initState() {
    super.initState();
    state.connect();
  }

  @override
  void dispose() {
    state.dispose();
    super.dispose();
  }

  Future<void> open(Widget screen) async {
    await Navigator.push(
      context,
      MaterialPageRoute<void>(builder: (_) => screen),
    );
    if (mounted) await state.refresh();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: state,
    builder: (context, _) => Scaffold(
      appBar: AppBar(title: const Text('Settings')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          ScreenStatus(state),
          const Text(
            'Reusable configurations. Conversations pick up saved settings on their next turn.',
          ),
          TextButton(
            onPressed: () => open(EndpointScreen(backend: widget.backend)),
            child: const Text('Add endpoint'),
          ),
          for (final e in state.snapshot?.endpoints ?? <EndpointConfig>[])
            ListTile(
              title: Text('${e.name} • ${e.id}'),
              subtitle: Text(e.model),
              onTap: () =>
                  open(EndpointScreen(backend: widget.backend, endpoint: e)),
            ),
          TextButton(
            onPressed: () => open(ServerScreen(backend: widget.backend)),
            child: const Text('Add MCP server'),
          ),
          for (final s in state.snapshot?.servers ?? <ServerConfig>[])
            ListTile(
              title: Text('${s.name} • ${s.id}'),
              onTap: () =>
                  open(ServerScreen(backend: widget.backend, server: s)),
            ),
          TextButton(
            onPressed: () => open(ProfileScreen(backend: widget.backend)),
            child: const Text('Add profile'),
          ),
          for (final p in state.snapshot?.profiles ?? <Profile>[])
            ListTile(
              title: Text('${p.name} • ${p.id}'),
              onTap: () =>
                  open(ProfileScreen(backend: widget.backend, profile: p)),
            ),
          TextButton(
            onPressed: () => open(ToolRulesScreen(backend: widget.backend)),
            child: const Text('Tool permissions'),
          ),
        ],
      ),
    ),
  );
}
