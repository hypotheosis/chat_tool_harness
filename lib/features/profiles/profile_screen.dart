import 'package:flutter/material.dart';
import '../../backend/backend.dart';
import '../../backend/models.dart';
import '../screen_controller.dart';

class ProfileScreen extends StatefulWidget {
  const ProfileScreen({super.key, required this.backend, this.profile});
  final Backend backend;
  final Profile? profile;
  @override
  State<ProfileScreen> createState() => _ProfileScreenState();
}

class _ProfileScreenState extends State<ProfileScreen> {
  late final state = ScreenController(widget.backend);
  late final name = TextEditingController(text: widget.profile?.name ?? '');
  late final instructions = TextEditingController(
    text: widget.profile?.instructions ?? '',
  );
  late final limits = [
    TextEditingController(
      text: '${widget.profile?.limits.maxInferenceRequests ?? 20}',
    ),
    TextEditingController(
      text: '${widget.profile?.limits.maxToolAttempts ?? 40}',
    ),
    TextEditingController(
      text: '${widget.profile?.limits.activeTimeoutMs ?? 900000}',
    ),
  ];
  late String? endpointId = widget.profile?.endpointId;
  late final servers = {...?widget.profile?.serverIds};
  late ProfileMode mode = widget.profile?.mode ?? ProfileMode.agent;
  String? result;
  @override
  void initState() {
    super.initState();
    state.connect();
  }

  @override
  void dispose() {
    state.dispose();
    name.dispose();
    instructions.dispose();
    for (final f in limits) {
      f.dispose();
    }
    super.dispose();
  }

  Future<void> save() async {
    final values = limits.map((v) => int.tryParse(v.text)).toList();
    if (endpointId == null || values.any((v) => v == null)) {
      state.error = failureMessage(
        const BackendFailure(FailureCode.invalidInput),
      );
      state.changed();
      return;
    }
    final id = widget.profile?.id ?? (await state.perform(const NewId()))?.id;
    if (id == null) return;
    final reply = await state.perform(
      SaveProfile(
        Profile(
          id: id,
          name: name.text,
          endpointId: endpointId!,
          serverIds: servers.toList(),
          instructions: instructions.text,
          mode: mode,
          limits: RunLimits(
            maxInferenceRequests: values[0]!,
            maxToolAttempts: values[1]!,
            activeTimeoutMs: values[2]!,
          ),
        ),
      ),
    );
    if (mounted && reply != null) {
      Navigator.pop(context);
    }
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: state,
    builder: (context, _) => Scaffold(
      appBar: AppBar(title: const Text('Profile')),
      bottomNavigationBar: ScreenStatus(state),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          inputField('Name', name),
          const Text(
            'Saving never changes existing conversation snapshots. Apply current profile from chat to change future turns.',
          ),
          DropdownButtonFormField<String>(
            initialValue: endpointId,
            isExpanded: true,
            decoration: const InputDecoration(labelText: 'Endpoint'),
            items: [
              for (final e in state.snapshot?.endpoints ?? <EndpointConfig>[])
                DropdownMenuItem(
                  value: e.id,
                  child: Text(
                    '${e.name} • ${e.id}',
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
            ],
            onChanged: (v) => setState(() {
              endpointId = v;
            }),
          ),
          SwitchListTile(
            title: const Text('Chat only (no MCP or local tools)'),
            subtitle: const Text(
              'Explicit choice for models without tool support. Diagnostics never change this mode.',
            ),
            value: mode == ProfileMode.chatOnly,
            onChanged: (v) => setState(() {
              mode = v ? ProfileMode.chatOnly : ProfileMode.agent;
            }),
          ),
          const Text('MCP servers (remembered but not connected in Chat only)'),
          for (final s in state.snapshot?.servers ?? <ServerConfig>[])
            CheckboxListTile(
              title: Text('${s.name} • ${s.id}'),
              value: servers.contains(s.id),
              onChanged: (v) => setState(() {
                if (v == true) {
                  servers.add(s.id);
                } else {
                  servers.remove(s.id);
                }
              }),
            ),
          inputField('System instructions', instructions, lines: 4),
          for (final e in [
            'Maximum inference requests',
            'Maximum tool attempts',
            'Cumulative active timeout (ms)',
          ].asMap().entries)
            inputField(e.value, limits[e.key], number: true),
          FilledButton(
            onPressed: state.loading ? null : save,
            child: const Text('Save profile'),
          ),
          if (state.snapshot?.endpoints.isEmpty ?? false)
            const Text('Create an endpoint in Settings first.'),
        ],
      ),
    ),
  );
}
