import 'package:flutter/material.dart';
import '../../backend/models.dart';
import '../screen_controller.dart';
import 'file_controller.dart';

class FilePanel extends StatefulWidget {
  const FilePanel({super.key, required this.controller});
  final FileController controller;
  @override
  State<FilePanel> createState() => _FilePanelState();
}

class _FilePanelState extends State<FilePanel> {
  List<ArtifactSummary> artifacts = [];
  ArtifactSummary? selected;
  String? preview;
  String? error;
  String? result;
  bool loading = true;
  @override
  void initState() {
    super.initState();
    load();
  }

  Future<void> action(Future<void> Function() work) async {
    setState(() {
      loading = true;
      error = null;
      result = null;
    });
    try {
      await work();
    } catch (e) {
      if (mounted) error = failureMessage(e);
    } finally {
      if (mounted) {
        setState(() {
          loading = false;
        });
      }
    }
  }

  Future<void> load() => action(() async {
    final list = await widget.controller.list();
    if (mounted) artifacts = list;
  });
  Future<void> read(ArtifactSummary a) => action(() async {
    final text = await widget.controller.preview(a.id);
    if (mounted) {
      selected = a;
      preview = text;
    }
  });
  Future<void> export(bool share) async {
    final a = selected;
    if (a == null) return;
    await action(() async {
      final outcome = share
          ? await widget.controller.share(a.id)
          : await widget.controller.save(a.id);
      if (mounted) {
        result = switch (outcome) {
          DocumentResult.cancelled => 'Save cancelled; no file exported.',
          DocumentResult.saved => 'Saved by explicit user choice.',
          DocumentResult.shared =>
            'Share sheet opened by explicit user choice.',
        };
      }
    });
  }

  Future<void> delete() async {
    final a = selected;
    if (a == null) return;
    final confirm = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Delete generated artifact?'),
        content: Text(a.name),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Delete'),
          ),
        ],
      ),
    );
    if (confirm != true) return;
    await action(() async {
      await widget.controller.delete(a.id);
      final list = await widget.controller.list();
      if (mounted) {
        artifacts = list;
        selected = null;
        preview = null;
      }
    });
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: const Text('Generated artifacts')),
    body: ListView(
      padding: const EdgeInsets.all(16),
      children: [
        if (loading) const LinearProgressIndicator(),
        if (error != null) Text(error!),
        TextButton(
          onPressed: loading ? null : load,
          child: const Text('Refresh artifacts'),
        ),
        if (!loading && artifacts.isEmpty)
          const Text('No generated text artifacts yet.'),
        for (final a in artifacts)
          ListTile(
            title: Text(a.name),
            subtitle: Text('${a.bytes} bytes • ${a.id}'),
            onTap: loading ? null : () => read(a),
          ),
        if (preview != null) ...[
          const Text(
            'Literal text preview — no HTML, image loading or link navigation.',
          ),
          SelectableText(preview!),
          Wrap(
            spacing: 8,
            children: [
              FilledButton(
                onPressed: loading ? null : () => export(false),
                child: const Text('Save'),
              ),
              OutlinedButton(
                onPressed: loading ? null : () => export(true),
                child: const Text('Share'),
              ),
              TextButton(
                onPressed: loading ? null : delete,
                child: const Text('Delete artifact'),
              ),
            ],
          ),
        ],
        if (result != null) Text(result!),
      ],
    ),
  );
}
