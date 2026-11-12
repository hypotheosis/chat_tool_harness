import 'package:flutter/material.dart';
import '../backend/models.dart';

/// The runs of [conversationId] that must be stopped before it can be
/// deleted: those executing now or waiting for approval.
List<Run> runsToStop(Iterable<Run> runs, String conversationId) => [
  for (final r in runs)
    if (r.conversationId == conversationId &&
        (r.state == RunState.running || r.state == RunState.waitingApproval))
      r,
];

/// Whether a run of [conversationId] has a tool call whose outcome is not
/// known, which deletion must confirm explicitly.
bool hasUnknownOutcomes(Iterable<Run> runs, String conversationId) => runs.any(
  (r) =>
      r.conversationId == conversationId &&
      r.attempts.any(
        (a) =>
            a.state == AttemptState.dispatching ||
            a.state == AttemptState.outcomeUnknown,
      ),
);

/// Asks before deleting a conversation permanently; true when confirmed.
/// [unknownOutcomes] adds the warning that tool outcomes remain unknown.
Future<bool> confirmDeleteConversation(
  BuildContext context, {
  required bool unknownOutcomes,
}) async =>
    await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Delete conversation permanently?'),
        content: Text(
          '${unknownOutcomes ? 'Unknown tool outcomes remain. ' : ''}Delete messages and owned files only. Shared settings remain. Remote effects are not rolled back. Stop active work and wait before deletion.',
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
    ) ==
    true;
