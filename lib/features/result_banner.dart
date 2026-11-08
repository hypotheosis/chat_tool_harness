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

  /// The result last shown, to notice when a new one arrives.
  (String?, String?, BackendFailure?) shown = (null, null, null);

  @override
  void initState() {
    super.initState();
    // A lazy list rebuilds the banner when it scrolls back into range; the
    // result it shows then is old, so record it without scrolling.
    final s = widget.state;
    shown = (s.error, s.success, s.lastFailure);
  }

  @override
  void didUpdateWidget(ResultBanner oldWidget) {
    super.didUpdateWidget(oldWidget);
    noticeResult();
  }

  /// A new result starts with details collapsed and is scrolled into view,
  /// centred so it stays clear of the screen edges, even when the action
  /// button that produced it sits at the bottom of the list.
  void noticeResult() {
    final s = widget.state;
    final current = (s.error, s.success, s.lastFailure);
    if (current.$1 == shown.$1 &&
        current.$2 == shown.$2 &&
        identical(current.$3, shown.$3)) {
      return;
    }
    shown = current;
    details = false;
    if (s.error == null && s.success == null) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      Scrollable.ensureVisible(
        context,
        alignment: 0.5,
        duration: const Duration(milliseconds: 200),
      );
    });
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.state;
    final scheme = Theme.of(context).colorScheme;
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
                  if (s.loadFailed)
                    TextButton(
                      onPressed: s.loading ? null : s.refresh,
                      child: const Text('Reload'),
                    ),
                ],
              ),
              if (detail != null && details)
                SelectableText(
                  [
                    'Target: ${failureTargetLabel(detail)}',
                    'Step: ${failureStepLabel(detail.step)}',
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
