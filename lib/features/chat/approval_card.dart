import 'dart:convert';
import 'package:flutter/material.dart';
import '../../backend/models.dart';

/// The calls of [run] that still need the user's decision: proposed attempts
/// with an approval binding, while the run waits for approval.
List<Attempt> pendingDecisions(Run run) => [
  if (run.state == RunState.waitingApproval)
    for (final a in run.attempts)
      if (a.state == AttemptState.proposed && a.binding != null) a,
];

/// The display name of [serverId] for a call of [run]: the run's own server
/// configuration, else the current one in [snapshot], else the id. Null
/// means local files.
String serverLabel(String? serverId, Run run, AppSnapshot? snapshot) {
  if (serverId == null) return 'Local files';
  for (final s in [...run.profile.servers, ...?snapshot?.servers]) {
    if (s.id == serverId) return s.name;
  }
  return serverId;
}

/// A short readable state for a tool call.
String attemptStateLabel(Attempt a) => switch (a.state) {
  AttemptState.proposed => a.binding == null ? 'Queued' : 'Needs approval',
  AttemptState.approved => 'Approved',
  AttemptState.denied => 'Denied',
  AttemptState.dispatching => 'Running',
  AttemptState.succeeded => 'Done',
  AttemptState.failed => 'Failed',
  AttemptState.outcomeUnknown => 'Outcome unknown',
};

/// [json] indented for reading, or unchanged when it is not valid JSON.
String prettyJson(String json) {
  try {
    return const JsonEncoder.withIndent('  ').convert(jsonDecode(json));
  } on FormatException {
    return json;
  }
}

/// One card for every call of a run that needs a decision, with Approve all,
/// Deny all, a per-call Deny and, when every call is from one MCP server,
/// Always allow. Null callbacks disable the buttons.
class ApprovalCard extends StatelessWidget {
  const ApprovalCard({
    super.key,
    required this.runId,
    required this.calls,
    required this.serverName,
    this.onApproveAll,
    this.onDenyAll,
    this.onDeny,
    this.onAlwaysAllow,
  });
  final String runId;

  /// Pending attempts, each with a binding.
  final List<Attempt> calls;

  /// The display name of a call's server (null for local files).
  final String Function(String? serverId) serverName;
  final VoidCallback? onApproveAll;
  final VoidCallback? onDenyAll;
  final void Function(Attempt attempt)? onDeny;

  /// Null when the calls are not all from one MCP server, or while busy.
  final VoidCallback? onAlwaysAllow;

  /// The MCP server every call is from, or null.
  static String? soleServer(List<Attempt> calls) {
    final ids = {for (final a in calls) a.call.key.serverId};
    return ids.length == 1 ? ids.single : null;
  }

  @override
  Widget build(BuildContext context) {
    final sole = soleServer(calls);
    final count = calls.length;
    final tools = count == 1 ? '1 tool' : '$count tools';
    final title = {for (final a in calls) a.call.key.serverId}.length == 1
        ? '${serverName(calls.first.call.key.serverId)} wants to run $tools'
        : '$count tool calls need your approval';
    final theme = Theme.of(context);
    return Card(
      key: Key('approval-$runId'),
      color: theme.colorScheme.secondaryContainer,
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(title, style: theme.textTheme.titleSmall),
            for (final a in calls)
              _PendingCall(
                attempt: a,
                serverName: serverName(a.call.key.serverId),
                onDeny: onDeny == null ? null : () => onDeny!(a),
              ),
            const SizedBox(height: 8),
            Wrap(
              spacing: 8,
              runSpacing: 4,
              children: [
                FilledButton(
                  key: Key('approve-all-$runId'),
                  onPressed: onApproveAll,
                  child: const Text('Approve all'),
                ),
                OutlinedButton(
                  key: Key('deny-all-$runId'),
                  onPressed: onDenyAll,
                  child: const Text('Deny all'),
                ),
                if (sole != null)
                  TextButton(
                    key: Key('always-allow-$runId'),
                    onPressed: onAlwaysAllow,
                    child: Text('Always allow ${serverName(sole)}'),
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _PendingCall extends StatefulWidget {
  const _PendingCall({
    required this.attempt,
    required this.serverName,
    this.onDeny,
  });
  final Attempt attempt;
  final String serverName;
  final VoidCallback? onDeny;
  @override
  State<_PendingCall> createState() => _PendingCallState();
}

class _PendingCallState extends State<_PendingCall> {
  bool expanded = false;

  @override
  Widget build(BuildContext context) {
    final a = widget.attempt;
    // The arguments shown are the exact ones the approval binds.
    final arguments = prettyJson(
      a.binding?.argumentsJson ?? a.call.argumentsJson,
    );
    final long =
        arguments.length > 300 || '\n'.allMatches(arguments).length > 8;
    final shown = !long || expanded;
    return Padding(
      padding: const EdgeInsets.only(top: 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  '${a.call.key.name} · ${widget.serverName}',
                  style: Theme.of(context).textTheme.labelLarge,
                ),
              ),
              TextButton(
                key: Key('deny-${a.id}'),
                onPressed: widget.onDeny,
                child: const Text('Deny'),
              ),
            ],
          ),
          if (shown)
            ConstrainedBox(
              constraints: const BoxConstraints(maxHeight: 180),
              child: SingleChildScrollView(
                child: SelectableText(
                  arguments,
                  style: const TextStyle(fontFamily: 'monospace'),
                ),
              ),
            ),
          if (long)
            TextButton(
              onPressed: () => setState(() => expanded = !expanded),
              child: Text(expanded ? 'Hide arguments' : 'Show arguments'),
            ),
        ],
      ),
    );
  }
}
