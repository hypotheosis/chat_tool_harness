import 'package:flutter/material.dart';
import '../backend/models.dart';
import 'screen_controller.dart';

/// The outcome of a settings screen's last action: the failure sentence with
/// a Details toggle, the success message, or nothing.
class ResultBanner extends StatefulWidget {
  const ResultBanner({super.key, required this.state});
  final ScreenController state;
  @override
  State<ResultBanner> createState() => _ResultBannerState();
}

class _ResultBannerState extends State<ResultBanner> {
  bool details = false;
  BackendFailure? shownFailure;

  @override
  Widget build(BuildContext context) {
    final s = widget.state;
    final scheme = Theme.of(context).colorScheme;
    // A new failure starts with its details collapsed.
    if (!identical(shownFailure, s.lastFailure)) {
      shownFailure = s.lastFailure;
      details = false;
    }
    if (s.error case final error?) {
      final detail = s.lastFailure?.detail;
      return Card(
        color: scheme.errorContainer,
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(Icons.warning_amber, color: scheme.onErrorContainer),
                  const SizedBox(width: 8),
                  Text(
                    'Failed',
                    style: TextStyle(color: scheme.onErrorContainer),
                  ),
                ],
              ),
              const SizedBox(height: 4),
              SelectableText(
                error,
                style: TextStyle(color: scheme.onErrorContainer),
              ),
              Wrap(
                children: [
                  if (detail != null)
                    TextButton(
                      onPressed: () => setState(() => details = !details),
                      child: Text(details ? 'Hide details' : 'Details'),
                    ),
                  TextButton(
                    onPressed: s.loading ? null : s.refresh,
                    child: const Text('Refresh'),
                  ),
                ],
              ),
              if (detail != null && details)
                SelectableText(
                  [
                    'Target: ${detail.targetKind}'
                        '${detail.targetName == null ? '' : ' "${detail.targetName}"'}',
                    'Step: ${detail.step}',
                    if (detail.url case final url?) 'URL: $url',
                    if (detail.status case final status?)
                      'HTTP status: $status',
                    if (detail.reason case final reason?) 'Reason: $reason',
                  ].join('\n'),
                  style: TextStyle(
                    fontFamily: 'monospace',
                    color: scheme.onErrorContainer,
                  ),
                ),
            ],
          ),
        ),
      );
    }
    if (s.success case final success?) {
      return Card(
        child: ListTile(
          leading: const Icon(Icons.check_circle_outline),
          title: Text(success),
        ),
      );
    }
    return const SizedBox.shrink();
  }
}
