import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import '../../backend/models.dart';
import '../screen_controller.dart';

/// The one next step a stopped run's card offers.
enum RunAction {
  openSettings,
  reenterCredential,
  chooseContext,
  retry,
  resume,
  allow,
  stop,
}

/// Why a failed, paused or interrupted run stopped, and what to do next.
class RunIssue {
  const RunIssue(this.title, this.cause, this.action);
  final String title;

  /// One sentence naming the cause.
  final String cause;

  /// Null when nothing on the card may continue the run.
  final RunAction? action;
}

/// Whether a tool call's remote outcome is unknown, so the run must never be
/// resumed or retried.
bool hasUnresolvedAttempt(Run run) => run.attempts.any(
  (a) =>
      a.state == AttemptState.outcomeUnknown ||
      a.state == AttemptState.dispatching,
);

/// The host of a normalized `scheme://host[:port]` origin.
String originHost(String origin) {
  final host = Uri.tryParse(origin)?.host ?? '';
  return host.isEmpty ? origin : host;
}

String _reasonText(ContextChoiceReason reason) => switch (reason) {
  ContextChoiceReason.windowExceeded =>
    "The conversation is too long for the model's context window. Choose which messages to send.",
  ContextChoiceReason.rawLimit =>
    'The conversation is larger than the request size limit. Choose which messages to send.',
  ContextChoiceReason.invalidHistory =>
    'Part of the history cannot be sent as it is, such as an unfinished tool call. Choose which messages to send.',
  ContextChoiceReason.providerRejected =>
    'The provider rejected the request as too long. Choose which messages to send.',
  ContextChoiceReason.disclosureRequired =>
    'The destination needs your permission before messages are sent.',
};

/// The card for [run], or null for runs that show no card: running and
/// waiting runs show a status row, completed and cancelled runs nothing.
///
/// [originAcknowledged] says whether the run's inference origin is already
/// acknowledged, so a disclosure pause can simply resume. [fixed] says the
/// user has since acted on the card (returned from the settings it opened,
/// or applied a context selection), so a resumable run now offers Resume and
/// a failed turn offers Retry.
RunIssue? runIssue(
  Run run, {
  required bool originAcknowledged,
  bool fixed = false,
}) {
  final title = switch (run.state) {
    RunState.failed => 'Run failed',
    RunState.paused => 'Paused',
    RunState.interrupted => 'Interrupted',
    _ => null,
  };
  if (title == null) return null;
  final ended = run.state == RunState.failed;
  // A tool call whose remote outcome is unknown must never be repeated, so
  // nothing on the card may start more model work for this turn.
  if (hasUnresolvedAttempt(run)) {
    return ended
        ? RunIssue(
            title,
            "A tool call's outcome is unknown; check the remote system before continuing.",
            null,
          )
        : RunIssue(
            title,
            "A tool call's outcome is unknown, so this run cannot resume. Check the remote system, then stop the run.",
            RunAction.stop,
          );
  }
  final reason = run.context.usage?.reason;
  final origin = run.profile.inferenceOrigin;
  if (!ended &&
      reason == ContextChoiceReason.disclosureRequired &&
      origin != null &&
      !originAcknowledged) {
    return RunIssue(
      title,
      'Messages will be sent to ${originHost(origin)}. Allow to continue this run.',
      RunAction.allow,
    );
  }
  final code = run.failureCode ?? (ended ? FailureCode.protocol : null);
  String? cause;
  RunAction? action;
  if (code != null) {
    cause = failureMessage(BackendFailure(code, detail: run.failureDetail));
    action = switch (failureAction(code)) {
      FailureAction.openSettings => RunAction.openSettings,
      FailureAction.reenterCredential => RunAction.reenterCredential,
      FailureAction.chooseContext => RunAction.chooseContext,
      FailureAction.retry => ended ? RunAction.retry : RunAction.resume,
    };
  } else if (reason != null &&
      reason != ContextChoiceReason.disclosureRequired) {
    cause = _reasonText(reason);
    action = RunAction.chooseContext;
  }
  if (cause != null && action != null) {
    return RunIssue(title, cause, fixed ? _afterFix(run, action) : action);
  }
  return RunIssue(
    title,
    run.state == RunState.paused
        ? 'The run is paused. Resume continues it with the current settings.'
        : 'The run stopped before it finished. Resume continues it with the current settings.',
    RunAction.resume,
  );
}

/// The action once the user has acted on [action]: an unfinished run
/// resumes; a failed turn whose settings were opened retries its message.
/// Callers have already ruled out runs with an unresolved attempt.
RunAction _afterFix(Run run, RunAction action) {
  if (run.state != RunState.failed) return RunAction.resume;
  final settings =
      action == RunAction.openSettings || action == RunAction.reenterCredential;
  return settings && run.inputMessageId != null ? RunAction.retry : action;
}

/// The inline card for a failed, paused or interrupted run: a title, the
/// cause, at most one action button and a Details toggle. A [compact] card,
/// for an earlier run, is one muted line with Details and no action.
class RunCard extends StatefulWidget {
  const RunCard({
    super.key,
    required this.run,
    required this.issue,
    this.actionLabel,
    this.actionKey,
    this.onAction,
    this.compact = false,
  });
  final Run run;
  final RunIssue issue;
  final String? actionLabel;
  final Key? actionKey;

  /// Null while another action is in progress.
  final VoidCallback? onAction;
  final bool compact;
  @override
  State<RunCard> createState() => _RunCardState();
}

class _RunCardState extends State<RunCard> {
  bool details = false;

  String detailText() {
    final run = widget.run;
    final detail = run.failureDetail;
    final usage = run.context.usage;
    return [
      'Run id: ${run.id}',
      if (detail != null) ...[
        'Target: ${failureTargetLabel(detail)}',
        'Step: ${failureStepLabel(detail.step)}',
        if (detail.url case final url?) 'URL: $url',
        if (detail.status case final status?) 'HTTP status: $status',
        if (detail.reason case final reason?) 'Reason: $reason',
      ] else if (run.failure case final failure?)
        'Reason: $failure',
      if (usage != null)
        'Context: about ${usage.estimatedInputTokens} of ${usage.window} tokens',
    ].join('\n');
  }

  Widget detailsToggle() => TextButton(
    onPressed: () => setState(() => details = !details),
    child: Text(details ? 'Hide details' : 'Details'),
  );

  /// The full detail, selectable, with Copy for reporting it elsewhere.
  Widget detailsBlock(Color? color) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      SelectableText(
        detailText(),
        key: Key('run-details-${widget.run.id}'),
        style: TextStyle(fontFamily: 'monospace', color: color),
      ),
      TextButton.icon(
        key: Key('copy-details-${widget.run.id}'),
        icon: const Icon(Icons.copy, size: 18),
        label: const Text('Copy details'),
        onPressed: () async {
          await Clipboard.setData(ClipboardData(text: detailText()));
          if (!mounted) return;
          ScaffoldMessenger.maybeOf(
            context,
          )?.showSnackBar(const SnackBar(content: Text('Details copied')));
        },
      ),
    ],
  );

  Widget compact(BuildContext context) {
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;
    return Padding(
      key: Key('run-card-${widget.run.id}'),
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  '${widget.issue.title}: ${widget.issue.cause}',
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: Theme.of(
                    context,
                  ).textTheme.bodySmall?.copyWith(color: muted),
                ),
              ),
              detailsToggle(),
            ],
          ),
          if (details) detailsBlock(muted),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    if (widget.compact) return compact(context);
    final scheme = Theme.of(context).colorScheme;
    final failed = widget.run.state == RunState.failed;
    final foreground = failed ? scheme.onErrorContainer : null;
    final label = widget.actionLabel;
    return Card(
      key: Key('run-card-${widget.run.id}'),
      color: failed ? scheme.errorContainer : null,
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(
                  failed ? Icons.error_outline : Icons.pause_circle_outline,
                  color: foreground,
                ),
                const SizedBox(width: 8),
                Text(
                  widget.issue.title,
                  style: Theme.of(
                    context,
                  ).textTheme.titleSmall?.copyWith(color: foreground),
                ),
              ],
            ),
            const SizedBox(height: 4),
            // A long reason shows its start; Details has all of it.
            Text(
              widget.issue.cause,
              key: Key('run-cause-${widget.run.id}'),
              maxLines: 6,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(color: foreground),
            ),
            const SizedBox(height: 8),
            Wrap(
              spacing: 8,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                if (label != null)
                  FilledButton.tonal(
                    key: widget.actionKey,
                    onPressed: widget.onAction,
                    child: Text(label),
                  ),
                detailsToggle(),
              ],
            ),
            if (details) detailsBlock(foreground),
          ],
        ),
      ),
    );
  }
}
