import 'package:flutter/material.dart';
import '../../backend/backend.dart';
import '../../backend/models.dart';
import '../files/file_controller.dart';
import '../files/file_panel.dart';
import '../profiles/profile_screen.dart';
import '../result_banner.dart';
import '../screen_controller.dart';
import '../settings/endpoint_screen.dart';
import '../settings/server_screen.dart';
import 'approval_card.dart';
import 'chat_controller.dart';
import 'model_wait.dart';
import 'run_card.dart';

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

  /// Runs whose card the user has acted on (returned from the settings it
  /// opened, or applied a context selection), which now offer Resume, or
  /// Retry for a failed turn.
  final fixedRuns = <String>{};
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

  /// Opens the context chooser for [target], or the summary chooser when
  /// null; true when a selection was applied to [target].
  Future<bool> chooseContext(Run? target) async {
    final conversation = state.conversation;
    if (conversation == null) return false;
    final messages = <String>{...target?.context.selection?.messageIds ?? []};
    final summaries = <String>{...target?.context.selection?.summaryIds ?? []};
    final origin = (target?.profile ?? conversation.profile).inferenceOrigin;
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
                  if (origin != null)
                    Text(
                      'Selected content will be sent to ${originHost(origin)}.',
                    ),
                  const Text(
                    'Select complete tool-call/result pairs. Summaries are derived context, not originals.',
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
    if (choice == null) return false;
    if (choice.$2) {
      await state.summarize(choice.$1);
      return false;
    }
    await state.select(target!.id, choice.$1);
    return state.error == null;
  }

  void markFixed(Run run) {
    if (mounted) setState(() => fixedRuns.add(run.id));
  }

  /// Asks before stopping: Stop is local and cannot undo remote work.
  Future<void> confirmStop(Run run) async {
    final stop = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Stop this run?'),
        content: const Text(
          'Stop is local and best effort. Work already sent to a server is not rolled back.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            key: const Key('confirm-stop'),
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Stop'),
          ),
        ],
      ),
    );
    if (stop == true) await state.stop(run.id);
  }

  Future<void> open(Widget screen) async {
    await Navigator.push(
      context,
      MaterialPageRoute<void>(builder: (_) => screen),
    );
    if (mounted) await state.refresh();
  }

  /// The current profile behind [snapshot], for editing.
  Widget profileScreen(ProfileSnapshot snapshot) => ProfileScreen(
    backend: widget.backend,
    profile: state.snapshot?.profiles
        .where((p) => p.id == snapshot.profileId)
        .firstOrNull,
  );

  /// The current endpoint of [snapshot], for editing.
  Widget endpointScreen(ProfileSnapshot snapshot) => EndpointScreen(
    backend: widget.backend,
    endpoint:
        state.snapshot?.endpoints
            .where((e) => e.id == snapshot.endpoint.id)
            .firstOrNull ??
        snapshot.endpoint,
  );

  /// The settings screen of the target that failed in [run]: the endpoint or
  /// server it names (credential notes name the holder of the credential),
  /// or the profile when the target is not known.
  Widget targetSettings(Run run) {
    final detail = run.failureDetail;
    final profile = run.profile;
    switch (detail?.targetKind) {
      case 'inference':
        return endpointScreen(profile);
      case 'mcpServer':
        final named = profile.servers
            .where((s) => s.name == detail?.targetName)
            .firstOrNull;
        final current = state.snapshot?.servers
            .where((s) => s.id == named?.id)
            .firstOrNull;
        if (current ?? named case final server?) {
          return ServerScreen(backend: widget.backend, server: server);
        }
      case 'credential':
        // A credential note without a holder: the endpoint is the only
        // possible holder when no server has secret headers.
        if (profile.servers.every((s) => s.secretHeaders.isEmpty)) {
          return endpointScreen(profile);
        }
    }
    return profileScreen(profile);
  }

  String actionLabel(RunAction action, Run run) => switch (action) {
    RunAction.openSettings =>
      'Open ${run.failureDetail?.targetName ?? 'profile'} settings',
    RunAction.reenterCredential => 'Re-enter credential',
    RunAction.chooseContext => 'Choose context',
    RunAction.retry => 'Retry',
    RunAction.resume => 'Resume',
    RunAction.allow => 'Allow',
    RunAction.stop => 'Stop',
  };

  Future<void> act(RunAction action, Run run, Conversation c) async {
    switch (action) {
      case RunAction.openSettings || RunAction.reenterCredential:
        await open(targetSettings(run));
        markFixed(run);
      case RunAction.chooseContext:
        // An ended run cannot take a new selection; a summary shortens the
        // context for the next turn instead.
        if (run.state == RunState.failed) {
          await chooseContext(null);
        } else if (await chooseContext(run)) {
          markFixed(run);
        }
      case RunAction.retry:
        // A turn re-runs from its own input message; a summary has none and
        // reopens the chooser instead.
        final input = run.inputMessageId;
        if (input == null || c.messages.every((m) => m.id != input)) {
          await chooseContext(null);
        } else {
          await state.retry(run.id);
        }
      case RunAction.resume:
        await state.resume(run.id);
      case RunAction.allow:
        await state.allowAndResume(run);
      case RunAction.stop:
        await confirmStop(run);
    }
  }

  /// Whether [action] starts or continues a run, which the backend refuses
  /// in an archived conversation.
  static bool startsWork(RunAction? action) => switch (action) {
    RunAction.retry ||
    RunAction.resume ||
    RunAction.allow ||
    RunAction.chooseContext => true,
    RunAction.openSettings || RunAction.reenterCredential => false,
    RunAction.stop || null => false,
  };

  /// Whether [run]'s own origin is the conversation's acknowledged one.
  bool runOriginAcknowledged(Run run, Conversation c) =>
      c.originAcknowledged &&
      run.profile.inferenceOrigin == c.profile.inferenceOrigin;

  /// Each run's card issue, computed once per build.
  Map<String, RunIssue?> issuesFor(Conversation c) => {
    for (final r in state.runs)
      r.id: runIssue(
        r,
        originAcknowledged: runOriginAcknowledged(r, c),
        fixed: fixedRuns.contains(r.id),
      ),
  };

  /// A run's tool calls and its status: a compact row while it works, one
  /// card when it failed or paused, nothing once it succeeded. Every paused
  /// or interrupted run keeps its full card and action, since several can
  /// be unfinished at once; an ended (failed) run collapses to one muted
  /// line unless it is the conversation's latest run.
  Widget? runView(Run run, Conversation c, Map<String, RunIssue?> issues) {
    if (run.state == RunState.completed || run.state == RunState.cancelled) {
      return null;
    }
    final issue = issues[run.id];
    final collapsed =
        run.state == RunState.failed && state.runs.lastOrNull?.id != run.id;
    // An archived conversation refuses new work, so it offers no action that
    // would start or continue a run; settings and Stop stay.
    final action = collapsed || (c.archived && startsWork(issue?.action))
        ? null
        : issue?.action;
    final busy = state.loading;
    return Column(
      key: ValueKey('run-${run.id}'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (run.partialText.isNotEmpty && run.state != RunState.failed)
          SafeMarkdown(run.partialText),
        for (final a in run.attempts) attempt(a, run),
        if (issue != null)
          RunCard(
            run: run,
            issue: issue,
            compact: collapsed,
            actionLabel: action == null ? null : actionLabel(action, run),
            actionKey: Key(switch (action) {
              RunAction.resume => 'resume-${run.id}',
              RunAction.stop => 'stop-${run.id}',
              _ => 'run-action-${run.id}',
            }),
            onAction: busy || action == null ? null : () => act(action, run, c),
          )
        else
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Row(
              children: [
                if (run.state == RunState.running)
                  const SizedBox.square(
                    dimension: 16,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                else
                  const Icon(Icons.pending_actions, size: 16),
                const SizedBox(width: 8),
                Expanded(
                  child: switch (run.modelWait) {
                    final wait? when run.state == RunState.running =>
                      ModelWaitStatus(wait, key: Key('model-wait-${run.id}')),
                    _ => Text(
                      run.state == RunState.running
                          ? 'Working…'
                          : hasUnresolvedAttempt(run)
                          ? "Waiting — a tool call's outcome is unknown"
                          : 'Waiting for your approval',
                    ),
                  },
                ),
                // The approval card decides pending calls and resumes; Resume
                // is only for a run whose calls are all decided already.
                if (run.state == RunState.waitingApproval &&
                    !c.archived &&
                    decisions(run).isEmpty &&
                    !hasUnresolvedAttempt(run))
                  TextButton(
                    key: Key('resume-${run.id}'),
                    onPressed: busy ? null : () => state.resume(run.id),
                    child: const Text('Resume'),
                  ),
                OutlinedButton(
                  key: Key('stop-${run.id}'),
                  onPressed: busy ? null : () => confirmStop(run),
                  child: const Text('Stop'),
                ),
              ],
            ),
          ),
      ],
    );
  }

  /// Messages in order, each run placed after the messages of its turn,
  /// which start at its input message; runs without one come last.
  List<Widget> transcript(Conversation c, Map<String, RunIssue?> issues) {
    final byInput = <String, List<Run>>{};
    final loose = <Run>[];
    final ids = {for (final m in c.messages) m.id};
    for (final r in state.runs) {
      final input = r.inputMessageId;
      if (input != null && ids.contains(input)) {
        (byInput[input] ??= []).add(r);
      } else {
        loose.add(r);
      }
    }
    final out = <Widget>[];
    var pending = <Run>[];
    void flush() {
      for (final r in pending) {
        if (runView(r, c, issues) case final view?) out.add(view);
      }
      pending = [];
    }

    // Tool calls and their results are kept out of the conversation: each
    // stretch of them becomes one collapsed "Used N tools" card.
    var activity = <Message>[];
    void flushActivity() {
      if (activity.isNotEmpty) out.add(toolActivity(activity, c));
      activity = [];
    }

    for (final m in c.messages) {
      if (byInput[m.id] case final runs?) {
        flushActivity();
        flush();
        pending = runs;
      }
      final calls = m.role == 'assistant' && m.calls.isNotEmpty;
      if (m.role == 'tool' || calls) {
        // Text the model wrote alongside its calls stays in the conversation.
        if (calls && m.content.trim().isNotEmpty) {
          flushActivity();
          out.add(messageCard(m));
        }
        activity.add(m);
      } else {
        flushActivity();
        out.add(messageCard(m));
      }
    }
    flushActivity();
    flush();
    pending = loose;
    flush();
    return out;
  }

  /// One collapsed card for consecutive tool calls and results in [messages]:
  /// each call with its arguments and the result that answered it.
  Widget toolActivity(List<Message> messages, Conversation c) {
    final calls = [for (final m in messages) ...m.calls];
    final results = {
      for (final m in messages)
        if (m.role == 'tool' && m.callId != null) m.callId!: m.content,
    };
    final answered = {for (final call in calls) call.providerCallId};
    final orphans = [
      for (final m in messages)
        if (m.role == 'tool' && !answered.contains(m.callId)) m.content,
    ];
    final names = {for (final call in calls) call.key.name}.join(', ');
    final count = calls.isEmpty ? orphans.length : calls.length;
    final muted = Theme.of(context).textTheme.bodySmall;
    Widget section(String label, String text) => Padding(
      padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(label, style: Theme.of(context).textTheme.labelMedium),
          SelectableText(text, style: muted),
        ],
      ),
    );
    return Card(
      key: Key('tools-${messages.first.id}'),
      elevation: 0,
      child: ExpansionTile(
        dense: true,
        leading: const Icon(Icons.build_outlined, size: 18),
        title: Text('Used $count tool${count == 1 ? '' : 's'}'),
        subtitle: names.isEmpty ? null : Text(names, style: muted),
        children: [
          for (final call in calls)
            ExpansionTile(
              dense: true,
              title: Text(
                '${call.key.name} · ${serverName(call.key.serverId, c.profile)}',
              ),
              children: [
                section('Arguments', call.argumentsJson),
                section(
                  'Result',
                  results[call.providerCallId] ?? 'No result recorded.',
                ),
              ],
            ),
          for (final text in orphans) section('Result', text),
        ],
      ),
    );
  }

  /// The display name of [serverId] in [profile], else in the current
  /// configuration, else the id.
  String serverName(String? serverId, ProfileSnapshot profile) =>
      serverLabel(serverId, [...profile.servers, ...?state.snapshot?.servers]);

  /// Servers trusted now. The current configuration decides, since trust can
  /// change while a run waits; a run's own snapshot may be stale.
  Set<String> get trusted => trustedServerIds(state.snapshot?.servers ?? []);

  /// The calls of [run] the user must still decide.
  List<Attempt> decisions(Run run) => pendingDecisions(run, trusted: trusted);

  /// Asks before trusting [server]: trust covers tools it adds later.
  Future<bool> confirmTrust(String server) async =>
      await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text('Always allow $server?'),
          content: Text(trustExplanation(server)),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              key: const Key('confirm-trust'),
              onPressed: () => Navigator.pop(context, true),
              child: const Text('Always allow'),
            ),
          ],
        ),
      ) ==
      true;

  /// The approval card of a run with calls that need a decision.
  Widget approvalCard(Run run, List<Attempt> calls) {
    final busy = state.loading;
    final bindings = [for (final a in calls) a.binding!];
    final sole = ApprovalCard.soleServer(calls);
    return ApprovalCard(
      runId: run.id,
      calls: calls,
      serverName: (id) => serverName(id, run.profile),
      onApproveAll: busy ? null : () => state.decide(run.id, approve: bindings),
      onDenyAll: busy
          ? null
          : () => state.decide(run.id, deny: [for (final a in calls) a.id]),
      onDeny: busy ? null : (a) => state.decide(run.id, deny: [a.id]),
      onAlwaysAllow: busy || sole == null
          ? null
          : () async {
              if (await confirmTrust(serverName(sole, run.profile))) {
                await state.trustAndApprove(run.id, sole, bindings);
              }
            },
    );
  }

  /// A user or assistant message. Tool calls are shown by [toolActivity].
  Widget messageCard(Message m) => Card(
    child: Padding(
      padding: const EdgeInsets.all(12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(m.role, style: Theme.of(context).textTheme.labelLarge),
          SafeMarkdown(m.content),
        ],
      ),
    ),
  );

  /// Below this body height (dp) the header is hidden; see the body builder.
  static const compactHeight = 200.0;

  Widget header(Conversation c) => ListTile(
    key: const Key('chat-header'),
    dense: true,
    title: Text(c.profile.name),
    subtitle: Text(
      c.profile.mode == ProfileMode.chatOnly
          ? '${c.profile.endpoint.model} • Chat only'
          : c.profile.endpoint.model,
      key: const Key('chat-model'),
    ),
    trailing: IconButton(
      icon: const Icon(Icons.tune),
      tooltip: 'Profile settings',
      onPressed: () => open(profileScreen(c.profile)),
    ),
  );

  Future<void> sendMessage() async {
    final text = message.text;
    await state.send(text);
    if (mounted && state.error == null) message.clear();
  }

  /// The result banner, approval and disclosure cards and the composer, kept
  /// above the system navigation bar and within [maxHeight] (half the room
  /// the chat has after the keyboard). The cards scroll; the composer stays.
  Widget bottom(
    Conversation c,
    Map<String, RunIssue?> issues, {
    required double maxHeight,
  }) {
    final origin = c.profile.inferenceOrigin;
    final unacknowledged = origin != null && !c.originAcknowledged;
    // A run card already offers Allow when it asks for this origin (Allow
    // only comes from paused or interrupted runs, which never collapse).
    final runAsks = state.runs.any(
      (r) =>
          r.profile.inferenceOrigin == origin &&
          issues[r.id]?.action == RunAction.allow,
    );
    return SafeArea(
      top: false,
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: maxHeight),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(12, 4, 12, 8),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Flexible(
                child: SingleChildScrollView(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      ResultBanner(state: state),
                      // Deciding resumes the run, so a run with a call whose outcome
                      // is unknown gets no approval card; Stop is all it offers.
                      // Nor does an archived conversation, which refuses resumes.
                      for (final r in c.archived ? const <Run>[] : state.runs)
                        if (decisions(r) case final calls
                            when calls.isNotEmpty && !hasUnresolvedAttempt(r))
                          approvalCard(r, calls),
                      // The backend refuses new work in an archived conversation.
                      if (c.archived)
                        Card(
                          key: const Key('archived'),
                          child: Padding(
                            padding: const EdgeInsets.all(12),
                            child: Row(
                              children: [
                                const Expanded(
                                  child: Text(
                                    'Archived. Restore this conversation to send messages or continue its runs.',
                                  ),
                                ),
                                const SizedBox(width: 8),
                                FilledButton(
                                  key: const Key('restore-archived'),
                                  onPressed: state.loading
                                      ? null
                                      : state.restore,
                                  child: const Text('Restore'),
                                ),
                              ],
                            ),
                          ),
                        ),
                      if (unacknowledged && !runAsks)
                        Card(
                          key: const Key('disclosure'),
                          child: Padding(
                            padding: const EdgeInsets.all(12),
                            child: Row(
                              children: [
                                Expanded(
                                  child: Text(
                                    'Messages will be sent to ${originHost(origin)}.',
                                  ),
                                ),
                                const SizedBox(width: 8),
                                FilledButton(
                                  key: const Key('ack-new-work'),
                                  onPressed: state.loading
                                      ? null
                                      : () => state.acknowledgeOrigin(origin),
                                  child: const Text('Allow'),
                                ),
                              ],
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
              Row(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  Expanded(
                    child: TextField(
                      key: const Key('message'),
                      controller: message,
                      minLines: 1,
                      maxLines: 4,
                      decoration: const InputDecoration(
                        labelText: 'Message',
                        border: OutlineInputBorder(),
                        isDense: true,
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  FilledButton(
                    onPressed: state.loading || unacknowledged || c.archived
                        ? null
                        : sendMessage,
                    child: const Text('Send'),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  /// A transcript tool card: `tool · server · state`, with ids, destination
  /// and arguments under its details. Decisions live in the approval card.
  Widget attempt(Attempt a, Run run) {
    final key = a.call.key;
    return Card(
      child: ExpansionTile(
        title: Text(
          '${key.name} · ${serverName(key.serverId, run.profile)} · ${attemptStateLabel(a, unresolved: hasUnresolvedAttempt(run), trusted: trusted)}',
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
              'Profile ${key.profileId}\nServer ${key.serverId ?? 'Local files'}\nTool ${key.name}\nAttempt ${a.id}',
            ),
          ),
          for (final server in run.profile.servers)
            if (server.id == key.serverId)
              SelectableText('Destination: ${server.url}'),
          const Text('Requested arguments (immutable)'),
          SizedBox(
            height: 180,
            child: SingleChildScrollView(
              child: SelectableText(a.call.argumentsJson),
            ),
          ),
          if (a.binding case final binding?)
            if (binding.argumentsJson != a.call.argumentsJson)
              SelectableText(binding.argumentsJson),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: state,
    builder: (context, _) {
      final c = state.conversation;
      final issues = c == null ? const <String, RunIssue?>{} : issuesFor(c);
      return Scaffold(
        appBar: AppBar(
          title: const Text('Chat'),
          bottom: settingsProgress(state),
          actions: [
            if (c != null)
              IconButton(
                onPressed: state.loading ? null : () => chooseContext(null),
                tooltip: 'Summarize context',
                icon: const Icon(Icons.compress),
              ),
            if (widget.documents case final documents?)
              IconButton(
                onPressed: () => open(
                  FilePanel(
                    controller: FileController(
                      widget.backend,
                      documents,
                      widget.conversationId,
                    ),
                  ),
                ),
                tooltip: 'Generated artifacts',
                icon: const Icon(Icons.folder_outlined),
              ),
            IconButton(
              onPressed: state.refresh,
              tooltip: 'Refresh',
              icon: const Icon(Icons.refresh),
            ),
          ],
        ),
        body: LayoutBuilder(
          builder: (context, room) {
            // Too short for the header and the composer together (landscape
            // with the keyboard open): drop the header and let the bottom
            // area use all the room, so the composer stays on screen.
            final compact = room.maxHeight < compactHeight;
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (c != null && !compact) header(c),
                Expanded(
                  child: ListView(
                    padding: const EdgeInsets.symmetric(horizontal: 12),
                    children: [
                      if (c == null && !state.loading)
                        const Text(
                          'Conversation unavailable. Return home or refresh.',
                        ),
                      if (c != null) ...transcript(c, issues),
                    ],
                  ),
                ),
                if (c != null)
                  bottom(
                    c,
                    issues,
                    maxHeight: compact ? room.maxHeight : room.maxHeight * 0.5,
                  )
                else
                  SafeArea(
                    top: false,
                    child: Padding(
                      padding: const EdgeInsets.all(12),
                      child: ResultBanner(state: state),
                    ),
                  ),
              ],
            );
          },
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
