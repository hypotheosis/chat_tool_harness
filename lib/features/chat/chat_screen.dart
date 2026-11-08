import 'package:flutter/material.dart';
import '../../backend/backend.dart';
import '../../backend/models.dart';
import '../screen_controller.dart';
import '../settings/endpoint_screen.dart';
import '../files/file_controller.dart';
import '../files/file_panel.dart';
import 'chat_controller.dart';

class ChatScreen extends StatefulWidget {
  const ChatScreen({
    super.key,
    required this.backend,
    required this.conversationId,
    this.documents,
  });
  final Backend backend;
  final String conversationId;
  final AndroidDocumentAccess? documents;
  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  late final state = ChatController(widget.backend, widget.conversationId);
  final message = TextEditingController();
  @override
  void initState() {
    super.initState();
    state.connect();
  }

  @override
  void dispose() {
    state.dispose();
    message.dispose();
    super.dispose();
  }

  Future<void> chooseContext(Run? target) async {
    final conversation = state.conversation;
    if (conversation == null) return;
    final messages = <String>{...target?.context.selection?.messageIds ?? []};
    final summaries = <String>{...target?.context.selection?.summaryIds ?? []};
    final choice = await showDialog<(ContextSelection, bool)>(
      context: context,
      builder: (context) => StatefulBuilder(
        builder: (context, update) => AlertDialog(
          title: const Text('Explicit context selection'),
          content: SizedBox(
            width: 400,
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    disclosureLabel(
                      target == null ? 'New summary' : 'Run ${target.id}',
                      target?.profile ?? conversation.profile,
                    ),
                  ),
                  const Text(
                    'No content is silently removed. Select complete tool-call/result pairs. Summaries are derived context, not originals. Summary requests also transmit selected content to inference.',
                  ),
                  for (final m in conversation.messages)
                    CheckboxListTile(
                      title: Text('${m.role}: ${m.content}'),
                      value: messages.contains(m.id),
                      onChanged: (v) => update(() {
                        if (v == true) {
                          messages.add(m.id);
                        } else {
                          messages.remove(m.id);
                        }
                      }),
                    ),
                  for (final s in conversation.summaries)
                    CheckboxListTile(
                      title: Text('Summary ${s.id}: ${s.text}'),
                      subtitle: Text(
                        'Unresolved attempts: ${s.unresolvedAttempts.join(', ')}',
                      ),
                      value: summaries.contains(s.id),
                      onChanged: (v) => update(() {
                        if (v == true) {
                          summaries.add(s.id);
                        } else {
                          summaries.remove(s.id);
                        }
                      }),
                    ),
                ],
              ),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('Cancel'),
            ),
            if (target == null)
              TextButton(
                onPressed: () => Navigator.pop(context, (
                  ContextSelection(
                    messageIds: messages.toList(),
                    summaryIds: summaries.toList(),
                  ),
                  true,
                )),
                child: const Text('Request summary'),
              ),
            if (target != null)
              FilledButton(
                onPressed: () => Navigator.pop(context, (
                  ContextSelection(
                    messageIds: messages.toList(),
                    summaryIds: summaries.toList(),
                  ),
                  false,
                )),
                child: const Text('Apply selection'),
              ),
          ],
        ),
      ),
    );
    if (choice != null) {
      if (choice.$2) {
        await state.summarize(choice.$1);
      } else {
        await state.select(target!.id, choice.$1);
      }
    }
  }

  String disclosureLabel(String target, ProfileSnapshot profile) =>
      '$target disclosure • revision ${profile.revision}\nDestination: ${profile.inferenceDestination ?? 'Unavailable — repair configuration'}';

  Widget runPanel(Run run) {
    final usage = run.context.usage;
    return Column(
      key: ValueKey('run-${run.id}'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text('Run ${run.id} • ${run.state.name}'),
        if (run.failure != null)
          const Text(
            'Run failed. Check configuration or context; no automatic retry was made.',
          ),
        if (run.partialText.isNotEmpty) SafeMarkdown(run.partialText),
        for (final a in run.attempts) attempt(a, run),
        if (usage != null)
          Text(
            'Estimated input: ${usage.estimatedInputTokens} tokens (conservative UTF-8 bytes). Output reserve ${usage.outputReserve}; window ${usage.window}.\n${usage.provider == null ? 'Provider usage not reported.' : 'Provider usage: ${usage.provider!.promptTokens} input / ${usage.provider!.completionTokens} output.'}\n${usage.reason == null ? '' : 'Context choice required: ${usage.reason!.name}'}',
          ),
        Text(disclosureLabel('Run ${run.id}', run.profile)),
        const Text(
          'Resuming this immutable run may transmit selected messages, tool results and derived summaries to its displayed destination, even after applying an updated profile.',
        ),
        TextButton(
          key: Key('ack-${run.id}'),
          onPressed: state.loading
              ? null
              : () => state.acknowledgeInferenceDisclosure(run.profile),
          child: const Text('Acknowledge inference disclosure'),
        ),
        Wrap(
          spacing: 8,
          children: [
            TextButton(
              key: Key('context-${run.id}'),
              onPressed: state.loading ? null : () => chooseContext(run),
              child: const Text('Choose context'),
            ),
            if (run.state == RunState.running ||
                run.state == RunState.waitingApproval ||
                run.state == RunState.paused ||
                run.state == RunState.interrupted)
              OutlinedButton(
                key: Key('stop-${run.id}'),
                onPressed: state.loading ? null : () => state.stop(run.id),
                child: const Text('Stop'),
              ),
            if ((run.state == RunState.paused ||
                    run.state == RunState.waitingApproval ||
                    run.state == RunState.interrupted) &&
                !run.attempts.any(
                  (a) =>
                      a.state == AttemptState.outcomeUnknown ||
                      a.state == AttemptState.dispatching,
                ))
              OutlinedButton(
                key: Key('resume-${run.id}'),
                onPressed: state.loading ? null : () => state.resume(run.id),
                child: const Text('Resume'),
              ),
          ],
        ),
      ],
    );
  }

  Widget attempt(Attempt a, Run run) => Card(
    child: ExpansionTile(
      title: Text(
        'Tool ${a.call.key.name} • ${a.call.key.serverId ?? 'Local files'} • ${a.state.name}',
      ),
      subtitle: a.state == AttemptState.outcomeUnknown
          ? const Text(
              'Outcome unknown — never retry automatically. Reconcile remote state outside this run.',
            )
          : null,
      childrenPadding: const EdgeInsets.all(12),
      children: [
        Align(
          alignment: Alignment.centerLeft,
          child: SelectableText(
            'Profile ${a.call.key.profileId}\nServer ${a.call.key.serverId ?? 'Local files'}\nTool ${a.call.key.name}\nAttempt ${a.id}',
          ),
        ),
        for (final server in run.profile.servers)
          if (server.id == a.call.key.serverId)
            SelectableText('Destination: ${server.url}'),
        const Text('Requested arguments (immutable)'),
        SizedBox(
          height: 180,
          child: SingleChildScrollView(
            child: SelectableText(a.call.argumentsJson),
          ),
        ),
        if (a.binding case final binding?) ...[
          if (binding.argumentsJson != a.call.argumentsJson)
            SelectableText(binding.argumentsJson),
          if (a.state == AttemptState.proposed ||
              a.state == AttemptState.approved)
            Wrap(
              spacing: 8,
              children: [
                FilledButton(
                  onPressed: state.loading
                      ? null
                      : () => state.approve(binding),
                  child: const Text('Approve exact arguments'),
                ),
                OutlinedButton(
                  onPressed: state.loading ? null : () => state.deny(a.id),
                  child: const Text('Deny'),
                ),
              ],
            ),
        ],
      ],
    ),
  );
  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: state,
    builder: (context, _) {
      final c = state.conversation;
      return Scaffold(
        appBar: AppBar(
          title: const Text('Chat'),
          actions: [
            IconButton(
              onPressed: state.refresh,
              tooltip: 'Refresh',
              icon: const Icon(Icons.refresh),
            ),
          ],
        ),
        bottomNavigationBar: ScreenStatus(state),
        body: ListView(
          padding: const EdgeInsets.all(12),
          children: [
            if (c == null && !state.loading)
              const Text('Conversation unavailable. Return home or refresh.'),
            if (c != null) ...[
              Text(
                '${c.profile.name} • revision ${c.profile.revision} • ${c.profile.mode.name}',
              ),
              SelectableText(
                'Current profile model: ${c.profile.endpoint.model}',
              ),
              if (c.profile.mode == ProfileMode.chatOnly)
                const Text(
                  'Chat only: MCP and local tools are disabled. This mode cannot perform tool actions.',
                ),
              const Text(
                'Conversation profile is an immutable snapshot. Applying a current profile affects future turns, not existing runs.',
              ),
              Wrap(
                spacing: 8,
                children: [
                  TextButton(
                    onPressed: state.loading
                        ? null
                        : () => state.applyProfile(c.profile.profileId),
                    child: const Text('Apply current profile'),
                  ),
                  TextButton(
                    onPressed: () => Navigator.push(
                      context,
                      MaterialPageRoute<void>(
                        builder: (_) => EndpointScreen(
                          backend: widget.backend,
                          endpoint: c.profile.endpoint,
                        ),
                      ),
                    ),
                    child: const Text('Repair configuration'),
                  ),
                ],
              ),
              for (final m in c.messages)
                Card(
                  child: Padding(
                    padding: const EdgeInsets.all(12),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          m.role,
                          style: Theme.of(context).textTheme.labelLarge,
                        ),
                        SafeMarkdown(m.content),
                        for (final call in m.calls)
                          ExpansionTile(
                            title: Text(
                              '${call.key.serverId ?? 'Local files'} / ${call.key.name}',
                            ),
                            children: [SelectableText(call.argumentsJson)],
                          ),
                      ],
                    ),
                  ),
                ),
              for (final r in state.runs) runPanel(r),
              Text(disclosureLabel('New turn / summary', c.profile)),
              const Text(
                'Selected messages, tool results and derived summaries may be transmitted to this exact destination. Acknowledgement is scoped to the displayed revision; it does not acknowledge other runs or updated profiles.',
              ),
              TextButton(
                key: const Key('ack-new-work'),
                onPressed: state.loading
                    ? null
                    : () => state.acknowledgeInferenceDisclosure(c.profile),
                child: const Text('Acknowledge new turn / summary disclosure'),
              ),
              TextButton(
                key: const Key('summary-context'),
                onPressed: state.loading ? null : () => chooseContext(null),
                child: const Text('Choose summary context'),
              ),
              const Text(
                'Stop is local / best effort, not remote rollback. Unknown outcomes are never retried.',
              ),
              TextField(
                key: const Key('message'),
                controller: message,
                minLines: 1,
                maxLines: 6,
                decoration: const InputDecoration(
                  labelText: 'Message',
                  border: OutlineInputBorder(),
                ),
              ),
              FilledButton(
                onPressed: state.loading
                    ? null
                    : () async {
                        final text = message.text;
                        await state.send(text);
                        if (mounted && state.error == null) message.clear();
                      },
                child: const Text('Send'),
              ),
              if (widget.documents != null)
                OutlinedButton(
                  onPressed: () => Navigator.push(
                    context,
                    MaterialPageRoute<void>(
                      builder: (_) => FilePanel(
                        controller: FileController(
                          widget.backend,
                          widget.documents!,
                          widget.conversationId,
                        ),
                      ),
                    ),
                  ),
                  child: const Text('Generated artifacts'),
                ),
            ],
          ],
        ),
      );
    },
  );
}

/// Text-only Markdown subset. Never creates navigable links, images or HTML widgets.
class SafeMarkdown extends StatelessWidget {
  const SafeMarkdown(this.text, {super.key});
  final String text;
  static final _inline = RegExp(r'\*\*[^*]+\*\*|`[^`]+`|\*[^*]+\*');
  Widget inline(String value, TextStyle? base) {
    final matches = _inline.allMatches(value).toList();
    if (matches.isEmpty) return SelectableText(value, style: base);
    final spans = <TextSpan>[];
    var cursor = 0;
    for (final match in matches) {
      if (match.start > cursor) {
        spans.add(TextSpan(text: value.substring(cursor, match.start)));
      }
      final part = match.group(0)!;
      final bold = part.startsWith('**');
      final code = part.startsWith('`');
      spans.add(
        TextSpan(
          text: part.substring(bold ? 2 : 1, part.length - (bold ? 2 : 1)),
          style: TextStyle(
            fontWeight: bold ? FontWeight.bold : null,
            fontFamily: code ? 'monospace' : null,
            fontStyle: !bold && !code ? FontStyle.italic : null,
          ),
        ),
      );
      cursor = match.end;
    }
    if (cursor < value.length) {
      spans.add(TextSpan(text: value.substring(cursor)));
    }
    return SelectableText.rich(TextSpan(style: base, children: spans));
  }

  @override
  Widget build(BuildContext context) {
    if (!text.contains('\n') && !text.startsWith('#')) {
      return inline(text, null);
    }
    final children = <Widget>[];
    var fenced = false;
    for (final line in text.split('\n')) {
      if (line.startsWith('```')) {
        fenced = !fenced;
        continue;
      }
      if (fenced) {
        children.add(
          SelectableText(line, style: const TextStyle(fontFamily: 'monospace')),
        );
        continue;
      }
      final heading = RegExp(r'^#{1,6} ').firstMatch(line);
      final value = heading != null
          ? line.substring(heading.end)
          : line.startsWith('- ') || line.startsWith('* ')
          ? '• ${line.substring(2)}'
          : line;
      children.add(
        inline(
          value,
          heading != null ? Theme.of(context).textTheme.titleLarge : null,
        ),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: children,
    );
  }
}
