import 'package:flutter/material.dart';
import '../../backend/models.dart';
import '../result_banner.dart';
import '../screen_controller.dart';

/// The outcome line of a model discovery that found [count] models.
String modelCountText(int count) => switch (count) {
  0 => 'No models available; enter the model ID manually.',
  1 => '1 model available',
  final n => '$n models available',
};

/// The outcome line of a tool-support probe.
String toolSupportText(ToolSupport? support) => switch (support) {
  ToolSupport.supported => 'Supported: structured tool calling verified.',
  ToolSupport.unsupported =>
    'Unsupported: choose Chat only explicitly in a profile. Profiles were not changed.',
  _ =>
    'Inconclusive: no verified structured tool call. Profiles were not changed.',
};

/// [at] in local time, such as `2026-11-10 14:03`.
String testedAtLabel(DateTime at) {
  final local = at.toLocal();
  String two(int n) => n.toString().padLeft(2, '0');
  return '${local.year}-${two(local.month)}-${two(local.day)} '
      '${two(local.hour)}:${two(local.minute)}';
}

/// The one-line outcome of [test]: what it found, or why it failed.
String connectionTestSummary(ConnectionTest test) {
  if (test.failure case final failure?) return failureMessage(failure);
  return switch (test.kind) {
    ConnectionTestKind.discoverModels => modelCountText(test.modelCount ?? 0),
    ConnectionTestKind.probeTools => toolSupportText(test.toolSupport),
    ConnectionTestKind.discoverServer => discoverySummary(
      test.toolCount ?? 0,
      test.skippedTools.length,
    ),
  };
}

/// The stored result of the last [label] test, shown until a new test
/// replaces it; nothing when the target was never tested.
class LastConnectionTest extends StatelessWidget {
  const LastConnectionTest({
    super.key,
    required this.label,
    required this.test,
    this.showSkipped = true,
  });
  final String label;
  final ConnectionTest? test;

  /// Whether to list skipped tools; off while the screen lists them from a
  /// test it just ran.
  final bool showSkipped;
  @override
  Widget build(BuildContext context) {
    final test = this.test;
    if (test == null) return const SizedBox.shrink();
    final failed = test.failure != null;
    final theme = Theme.of(context);
    return Card(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(
                  failed ? Icons.error_outline : Icons.history,
                  color: failed
                      ? theme.colorScheme.error
                      : theme.colorScheme.onSurfaceVariant,
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    '$label last tested ${testedAtLabel(test.testedAt)}'
                    '${failed ? ': failed' : ''}',
                    style: theme.textTheme.titleSmall,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
            SelectableText(connectionTestSummary(test)),
            if (showSkipped) SkippedToolList(test.skippedTools),
          ],
        ),
      ),
    );
  }
}
