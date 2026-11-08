import 'package:flutter/material.dart';
import '../../backend/backend.dart';
import '../../backend/models.dart';
import '../chat/chat_screen.dart';
import '../files/file_controller.dart';
import 'history_controller.dart';

class HistoryScreen extends StatefulWidget {
  const HistoryScreen({super.key, required this.backend, this.documents});
  final Backend backend;
  final AndroidDocumentAccess? documents;
  @override
  State<HistoryScreen> createState() => _HistoryScreenState();
}

class _HistoryScreenState extends State<HistoryScreen> {
  late final state = HistoryController(
    widget.backend,
    documents: widget.documents,
  );
  final query = TextEditingController();
  @override
  void initState() {
    super.initState();
    state.connect();
  }

  @override
  void dispose() {
    query.dispose();
    state.dispose();
    super.dispose();
  }

  Future<void> rename(ConversationSummary row) async {
    final title = TextEditingController(text: row.title);
    final value = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Rename conversation'),
        content: TextField(
          key: const Key('history-title'),
          controller: title,
          maxLength: 256,
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, title.text),
            child: const Text('Rename'),
          ),
        ],
      ),
    );
    // Dialog route animation may still hold the controller for one frame.
    if (mounted && value != null) await state.rename(row.id, value);
    WidgetsBinding.instance.addPostFrameCallback((_) => title.dispose());
  }

  Future<void> delete(ConversationSummary row) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Delete conversation permanently?'),
        content: Text(
          '${row.hasUnknownOutcomes ? 'Unknown tool outcomes remain. ' : ''}Delete messages and owned files only. Shared settings remain. Remote effects are not rolled back. Stop active work and wait before deletion.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Confirm delete'),
          ),
        ],
      ),
    );
    if (mounted && confirmed == true) {
      await state.delete(row.id, confirmedUnknown: row.hasUnknownOutcomes);
    }
  }

  Future<void> export(ConversationSummary row) async {
    state.clearPreview();
    await showDialog<void>(
      context: context,
      builder: (_) => _ExportDialog(state: state, owner: row.id),
    );
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: state,
    builder: (context, _) => Scaffold(
      appBar: AppBar(title: const Text('History')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          const Text(
            'Search titles and message text only. No attachment/document text search.',
          ),
          TextField(
            key: const Key('history-search'),
            controller: query,
            decoration: const InputDecoration(labelText: 'Literal search'),
            onSubmitted: state.loading ? null : (v) => state.search(v),
          ),
          FilledButton(
            onPressed: state.loading ? null : () => state.search(query.text),
            child: const Text('Search'),
          ),
          if (state.loading) const LinearProgressIndicator(),
          if (state.error != null) Text(state.error!),
          if (state.rows.isEmpty && !state.loading)
            const Text('No conversations found.'),
          for (final row in state.rows)
            Card(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  ListTile(
                    title: Text(row.title),
                    subtitle: row.hasUnknownOutcomes
                        ? const Text(
                            'Unknown tool outcomes — no remote rollback',
                          )
                        : null,
                    onTap: state.loading
                        ? null
                        : () async {
                            await Navigator.push(
                              context,
                              MaterialPageRoute<void>(
                                builder: (_) => ChatScreen(
                                  backend: widget.backend,
                                  conversationId: row.id,
                                  documents: widget.documents,
                                ),
                              ),
                            );
                            if (mounted) {
                              await state.search(
                                query.text,
                                pageOffset: state.offset,
                              );
                            }
                          },
                  ),
                  Wrap(
                    children: [
                      IconButton(
                        tooltip: 'Rename',
                        onPressed: state.loading ? null : () => rename(row),
                        icon: const Icon(Icons.edit),
                      ),
                      IconButton(
                        tooltip: 'Delete',
                        onPressed: state.loading || state.needsStop(row.id)
                            ? null
                            : () => delete(row),
                        icon: const Icon(Icons.delete),
                      ),
                      IconButton(
                        tooltip: 'Export transcript',
                        onPressed: state.loading ? null : () => export(row),
                        icon: const Icon(Icons.ios_share),
                      ),
                      if (state.needsStop(row.id))
                        TextButton(
                          onPressed: state.loading
                              ? null
                              : () => state.stop(row.id),
                          child: const Text('Stop and wait'),
                        ),
                    ],
                  ),
                ],
              ),
            ),
          Row(
            children: [
              TextButton(
                onPressed: state.loading || state.offset == 0
                    ? null
                    : state.previousPage,
                child: const Text('Previous'),
              ),
              Text('Page ${state.offset ~/ 100 + 1}'),
              TextButton(
                onPressed: state.loading || !state.hasNext
                    ? null
                    : state.nextPage,
                child: const Text('Next'),
              ),
            ],
          ),
        ],
      ),
    ),
  );
}

class _ExportDialog extends StatefulWidget {
  const _ExportDialog({required this.state, required this.owner});
  final HistoryController state;
  final String owner;
  @override
  State<_ExportDialog> createState() => _ExportDialogState();
}

class _ExportDialogState extends State<_ExportDialog> {
  bool details = false;
  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.state,
    builder: (context, _) {
      final state = widget.state;
      return AlertDialog(
        title: const Text('Export transcript'),
        content: SizedBox(
          width: 600,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  'Transcript, not a restorable backup. Configured secrets are redacted; unknown or user-entered sensitive content and unrecognized encodings may remain. Review before sharing. Tool summaries by default. No attachment files.',
                ),
                CheckboxListTile(
                  value: details,
                  onChanged: state.loading
                      ? null
                      : (v) {
                          setState(() => details = v!);
                          state.clearPreview();
                        },
                  title: const Text(
                    'Include detailed tool payloads (sensitive)',
                  ),
                ),
                if (state.loading) const LinearProgressIndicator(),
                if (state.error != null) Text(state.error!),
                if (state.transcript != null)
                  SelectableText(state.transcript!.text),
                if (state.delivery != null) Text(state.delivery!),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: state.loading ? null : () => Navigator.pop(context),
            child: const Text('Close'),
          ),
          TextButton(
            onPressed: state.loading
                ? null
                : () => state.preview(widget.owner, details),
            child: const Text('Preview'),
          ),
          TextButton(
            onPressed:
                state.loading ||
                    state.transcript == null ||
                    state.documents == null
                ? null
                : () => state.deliver(false),
            child: const Text('Save'),
          ),
          TextButton(
            onPressed:
                state.loading ||
                    state.transcript == null ||
                    state.documents == null
                ? null
                : () => state.deliver(true),
            child: const Text('Share'),
          ),
        ],
      );
    },
  );
}
