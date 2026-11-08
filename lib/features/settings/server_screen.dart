import 'package:flutter/material.dart';
import 'credential_editor.dart';
import '../../backend/backend.dart';
import '../../backend/models.dart';
import '../result_banner.dart';
import '../screen_controller.dart';

class ServerScreen extends StatefulWidget {
  const ServerScreen({super.key, required this.backend, this.server});
  final Backend backend;
  final ServerConfig? server;
  @override
  State<ServerScreen> createState() => _ServerScreenState();
}

class _ServerScreenState extends State<ServerScreen> {
  late final state = ScreenController(widget.backend);
  late String? id = widget.server?.id;
  late final name = TextEditingController(text: widget.server?.name ?? '');
  late final url = TextEditingController(text: widget.server?.url ?? '');
  late final headers = [
    for (final h in widget.server?.secretHeaders ?? <SecretHeader>[])
      (
        TextEditingController(text: h.name),
        TextEditingController(text: h.credential),
      ),
  ];
  late bool http = widget.server?.allowHttp ?? false;
  String? profileId;
  List<String> tools = [];
  @override
  void initState() {
    super.initState();
    state.connect();
  }

  @override
  void dispose() {
    state.dispose();
    name.dispose();
    url.dispose();
    for (final h in headers) {
      h.$1.dispose();
      h.$2.dispose();
    }
    super.dispose();
  }

  Future<void> save() async {
    id ??= (await state.perform(const NewId()))?.id;
    if (id == null) return;
    final reply = await state.perform(
      SaveServer(
        ServerConfig(
          id: id!,
          name: name.text,
          url: url.text,
          allowHttp: http,
          secretHeaders: [
            for (final h in headers)
              SecretHeader(name: h.$1.text, credential: h.$2.text),
          ],
        ),
      ),
    );
    if (reply != null && mounted) {
      state.succeed('Server saved.');
    }
  }

  Future<void> discover() async {
    if (id == null || profileId == null) {
      state.reject(
        'Save the server and select a saved profile containing it first.',
      );
      return;
    }
    setState(() {
      tools = [];
    });
    final reply = await state.perform(DiscoverServer(profileId!, id!));
    if (!mounted || reply == null) return;
    setState(() {
      tools = reply.discoveredTools;
    });
    final count = tools.length == 1 ? '1 tool' : '${tools.length} tools';
    state.succeed('Connected to ${name.text}: $count available');
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: state,
    builder: (context, _) => Scaffold(
      appBar: AppBar(title: const Text('MCP server')),
      bottomNavigationBar: ScreenStatus(state, showError: false),
      body: SafeArea(
        top: false,
        child: ListView(
          padding: settingsListPadding(context),
          children: [
            inputField('Name', name),
            inputField('Streamable HTTP URL', url),
            SwitchListTile(
              title: const Text('Allow insecure HTTP for this server'),
              subtitle: const Text(
                'Warning: credentials and payloads travel unencrypted over HTTP.',
              ),
              value: http,
              onChanged: state.loading
                  ? null
                  : (v) => setState(() {
                      http = v;
                    }),
            ),
            const Text(
              'Secret headers use credential-reference UUIDs only, never raw tokens.',
            ),
            for (final h in headers)
              Column(
                key: ObjectKey(h.$2),
                children: [
                  inputField('Header name', h.$1),
                  inputField('Credential reference UUID', h.$2),
                  CredentialEditor(
                    backend: widget.backend,
                    reference: h.$2,
                    headerName: h.$1,
                  ),
                  TextButton(
                    onPressed: () => setState(() {
                      headers.remove(h);
                      h.$1.dispose();
                      h.$2.dispose();
                    }),
                    child: const Text('Remove header'),
                  ),
                ],
              ),
            TextButton(
              onPressed: () => setState(() {
                headers.add((TextEditingController(), TextEditingController()));
              }),
              child: const Text('Add secret header reference'),
            ),
            FilledButton(
              onPressed: state.loading ? null : save,
              child: const Text('Save server'),
            ),
            const Text(
              'Safe connectivity/discovery only. Select a saved profile that includes this server; save edits before testing.',
            ),
            DropdownButtonFormField<String>(
              initialValue: profileId,
              isExpanded: true,
              decoration: const InputDecoration(labelText: 'Linked profile'),
              items: [
                for (final p in state.snapshot?.profiles ?? <Profile>[])
                  if (p.serverIds.contains(id))
                    DropdownMenuItem(
                      value: p.id,
                      child: Text(
                        '${p.name} • ${p.id}',
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
              ],
              onChanged: (v) => setState(() {
                profileId = v;
              }),
            ),
            OutlinedButton(
              onPressed: state.loading ? null : discover,
              child: const Text('Test connection and discover tools'),
            ),
            ResultBanner(state: state),
            for (final tool in tools) SelectableText(tool),
          ],
        ),
      ),
    ),
  );
}
