import 'package:flutter/material.dart';
import 'credential_editor.dart';
import '../../backend/backend.dart';
import '../../backend/models.dart';
import '../screen_controller.dart';

class EndpointScreen extends StatefulWidget {
  const EndpointScreen({super.key, required this.backend, this.endpoint});
  final Backend backend;
  final EndpointConfig? endpoint;
  @override
  State<EndpointScreen> createState() => _EndpointScreenState();
}

class _EndpointScreenState extends State<EndpointScreen> {
  late final state = ScreenController(widget.backend);
  late String? id = widget.endpoint?.id;
  late final fields = [
    TextEditingController(text: widget.endpoint?.name ?? ''),
    TextEditingController(text: widget.endpoint?.baseUrl ?? ''),
    TextEditingController(text: widget.endpoint?.model ?? ''),
    TextEditingController(text: widget.endpoint?.credential ?? ''),
    TextEditingController(text: '${widget.endpoint?.contextTokens ?? 4096}'),
    TextEditingController(text: '${widget.endpoint?.maxOutputTokens ?? 512}'),
    TextEditingController(text: '${widget.endpoint?.temperature ?? 0.2}'),
  ];
  late bool http = widget.endpoint?.allowHttp ?? false;
  String? result;
  List<String> models = [];
  @override
  void initState() {
    super.initState();
    state.connect();
  }

  @override
  void dispose() {
    state.dispose();
    for (final f in fields) {
      f.dispose();
    }
    super.dispose();
  }

  Future<void> save() async {
    final contextTokens = int.tryParse(fields[4].text);
    final output = int.tryParse(fields[5].text);
    final temp = double.tryParse(fields[6].text);
    if (contextTokens == null || output == null || temp == null) {
      state.error = failureMessage(
        const BackendFailure(FailureCode.invalidInput),
      );
      state.changed();
      return;
    }
    id ??= (await state.perform(const NewId()))?.id;
    if (id == null) return;
    final reply = await state.perform(
      SaveEndpoint(
        EndpointConfig(
          id: id!,
          name: fields[0].text,
          baseUrl: fields[1].text,
          model: fields[2].text,
          credential: fields[3].text.trim().isEmpty
              ? null
              : fields[3].text.trim(),
          contextTokens: contextTokens,
          maxOutputTokens: output,
          temperature: temp,
          allowHttp: http,
        ),
      ),
    );
    if (mounted && reply != null) {
      setState(() {
        result = 'Endpoint saved. Existing conversations keep their snapshots.';
      });
    }
  }

  Future<void> diagnose(bool probe) async {
    if (id == null) {
      setState(() {
        result = 'Save the endpoint first.';
      });
      return;
    }
    setState(() {
      result = null;
      models = [];
    });
    final reply = await state.perform(
      probe ? ProbeEndpoint(id!) : DiscoverModels(id!),
    );
    if (!mounted || reply == null) return;
    setState(() {
      models = reply.models.toSet().toList();
      result = probe
          ? switch (reply.toolSupport) {
              ToolSupport.supported =>
                'Supported: structured tool calling verified.',
              ToolSupport.unsupported =>
                'Unsupported: choose Chat only explicitly in a profile. Profiles were not changed.',
              _ =>
                'Inconclusive: no verified structured tool call. Profiles were not changed.',
            }
          : models.isEmpty
          ? 'No models discovered; manual model ID remains available.'
          : 'Choose a discovered model or enter one manually.';
    });
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: state,
    builder: (context, _) => Scaffold(
      appBar: AppBar(title: const Text('Inference endpoint')),
      bottomNavigationBar: ScreenStatus(state),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          const Text(
            'Paste tokens only into the secure credential editor below. Diagnostics use the saved endpoint and transmit only a fixed diagnostic prompt, never conversation content.',
          ),
          for (final entry in [
            'Name',
            'Base URL',
            'Model ID',
            'Credential reference (UUID, not a token)',
            'Context window tokens',
            'Maximum output tokens',
            'Temperature',
          ].asMap().entries)
            inputField(entry.value, fields[entry.key], number: entry.key >= 4),
          CredentialEditor(backend: widget.backend, reference: fields[3]),
          SwitchListTile(
            title: const Text('Allow insecure HTTP for this endpoint'),
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
          FilledButton(
            onPressed: state.loading ? null : save,
            child: const Text('Save endpoint'),
          ),
          Wrap(
            spacing: 8,
            children: [
              OutlinedButton(
                onPressed: state.loading ? null : () => diagnose(false),
                child: const Text('Discover models'),
              ),
              OutlinedButton(
                onPressed: state.loading ? null : () => diagnose(true),
                child: const Text('Probe tool support'),
              ),
            ],
          ),
          if (result != null) Text(result!),
          for (final model in models)
            ListTile(
              title: Text(model),
              onTap: () {
                fields[2].text = model;
              },
            ),
        ],
      ),
    ),
  );
}
