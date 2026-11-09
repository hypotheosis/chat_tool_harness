import 'dart:async';
import 'package:flutter/material.dart';
import '../backend/backend.dart';
import '../backend/models.dart';

/// The one next step a failure offers the user.
enum FailureAction { openSettings, reenterCredential, chooseContext, retry }

FailureAction failureAction(FailureCode code) => switch (code) {
  FailureCode.credentialMissing => FailureAction.reenterCredential,
  FailureCode.authentication ||
  FailureCode.unavailableTransport ||
  FailureCode.protocol ||
  FailureCode.configuration ||
  FailureCode.toolUnsupported ||
  FailureCode.discoveryUnavailable => FailureAction.openSettings,
  FailureCode.contextLength => FailureAction.chooseContext,
  _ => FailureAction.retry,
};

/// A readable name for the failed target, such as `MCP server "tools"`.
String failureTargetLabel(FailureDetail detail) {
  final kind = switch (detail.targetKind) {
    'inference' => 'Inference endpoint',
    'mcpServer' => 'MCP server',
    'profile' => 'Profile',
    _ => 'Stored credential',
  };
  final name = detail.targetName;
  return name == null || name.isEmpty ? kind : '$kind "$name"';
}

/// A readable name for the failed step, such as `initialization`.
String failureStepLabel(String step) => switch (step) {
  'discoverModels' => 'model discovery',
  'chatRequest' => 'the chat request',
  'initialize' => 'initialization',
  'listTools' => 'tool listing',
  'callTool' => 'the tool call',
  'resolveCredential' => 'credential lookup',
  'refreshProfile' => 'profile refresh',
  _ => 'the request',
};

/// Plain text for a failure without detail.
String _codeText(FailureCode code) => switch (code) {
  FailureCode.authentication =>
    'Authentication failed: the server rejected the credential. Check or re-enter it in settings.',
  FailureCode.credentialMissing =>
    'The stored credential is missing. Re-enter it in settings.',
  FailureCode.invalidInput =>
    'Some input is invalid. Check required fields and numbers.',
  FailureCode.configuration =>
    'The configuration is incomplete. Check the endpoint, profile and servers.',
  FailureCode.toolUnsupported =>
    'This model does not support tool calling. Use Chat only mode in the profile.',
  FailureCode.discoveryUnavailable =>
    'Discovery is not available here. Enter the model ID manually or check the MCP server.',
  FailureCode.contextLength =>
    'The conversation is too long for the model. Choose context or summarize.',
  FailureCode.rateLimited =>
    'The provider is rate limiting. Nothing was retried; try again shortly.',
  FailureCode.cancelled => 'Stopped. Remote effects are not rolled back.',
  FailureCode.limit => 'A configured safety limit was reached.',
  FailureCode.storage => 'Local storage failed. Refresh and try again.',
  FailureCode.protocol => 'The server sent an unexpected response.',
  FailureCode.unavailableTransport =>
    'The service is unavailable or could not be reached. Check connectivity and the URL.',
  FailureCode.conflict =>
    'Something changed or is busy. No work was queued. Refresh and try again.',
  FailureCode.unsupportedPlatform =>
    'This is not available on this platform yet.',
};

/// The next step to append after a detailed failure sentence, which already
/// says what went wrong.
String _actionHint(FailureCode code) => switch (code) {
  FailureCode.authentication => 'Check or re-enter the credential in settings.',
  FailureCode.credentialMissing => 'Re-enter the credential in settings.',
  FailureCode.invalidInput => 'Check required fields and numbers.',
  FailureCode.configuration => 'Check the endpoint, profile and servers.',
  FailureCode.toolUnsupported => 'Use Chat only mode in the profile.',
  FailureCode.discoveryUnavailable => 'Enter the model ID manually.',
  FailureCode.contextLength => 'Choose context or summarize.',
  FailureCode.rateLimited => 'Nothing was retried; try again shortly.',
  FailureCode.cancelled => 'Remote effects are not rolled back.',
  FailureCode.limit => 'A configured safety limit was reached.',
  FailureCode.protocol => 'Check the URL and server type.',
  FailureCode.unavailableTransport => 'Check connectivity and the URL.',
  FailureCode.storage || FailureCode.conflict => 'Refresh and try again.',
  FailureCode.unsupportedPlatform => 'Use the Android app for this.',
};

/// One sentence naming what failed, from the detail when present, followed
/// by the next step; without detail, the plain per-code text. Non-[BackendFailure] errors never leak their
/// text; they read as an unavailable service.
String failureMessage(Object error, {bool approval = false}) {
  final failure = error is BackendFailure
      ? error
      : const BackendFailure(FailureCode.unavailableTransport);
  if (approval && failure.code == FailureCode.conflict) {
    return 'This is a stale approval or the policy changed. Refresh and review the current exact arguments.';
  }
  final detail = failure.detail;
  if (detail == null) return _codeText(failure.code);
  final status = detail.status;
  final what = status == null
      ? 'failed during ${failureStepLabel(detail.step)}'
      : 'HTTP $status during ${failureStepLabel(detail.step)}';
  var reason = detail.reason?.trim() ?? '';
  while (reason.endsWith('.')) {
    reason = reason.substring(0, reason.length - 1);
  }
  final because = reason.isEmpty ? '' : ': $reason';
  return '${failureTargetLabel(detail)}: $what$because. '
      '${_actionHint(failure.code)}';
}

class ScreenController extends ChangeNotifier {
  ScreenController(Backend backend) : session = BackendSession(backend);
  final BackendSession session;
  AppSnapshot? get snapshot => session.current;
  bool loading = false;
  String? error;

  /// The failure behind [error], when it came from the backend.
  BackendFailure? lastFailure;

  /// The outcome of the last successful action, shown by `ResultBanner`.
  String? success;

  /// Whether [error] came from loading the snapshot, which Reload retries.
  bool loadFailed = false;
  bool _disposed = false;
  StreamSubscription<AppSnapshot>? _subscription;
  void changed() {
    if (!_disposed) notifyListeners();
  }

  void _fail(Object e, {bool approval = false, bool load = false}) {
    error = failureMessage(e, approval: approval);
    lastFailure = e is BackendFailure ? e : null;
    success = null;
    loadFailed = load;
  }

  void _clearResult() {
    error = null;
    lastFailure = null;
    success = null;
    loadFailed = false;
  }

  /// Shows [message] as the outcome of the last action.
  void succeed(String message) {
    _clearResult();
    success = message;
    changed();
  }

  /// Shows [message] as a failure that did not come from the backend, such
  /// as a missing precondition.
  void reject(String message) {
    _clearResult();
    error = message;
    changed();
  }

  Future<void> connect() async {
    _subscription = session.changes.listen(
      (_) => changed(),
      onError: (Object e) {
        _fail(e, load: true);
        changed();
      },
    );
    loading = true;
    changed();
    try {
      await session.connect();
    } catch (e) {
      _fail(e, load: true);
    } finally {
      loading = false;
      changed();
    }
  }

  Future<void> refresh() async {
    if (_disposed || loading) return;
    _clearResult();
    loading = true;
    changed();
    try {
      await session.connect();
    } catch (e) {
      _fail(e, load: true);
    } finally {
      loading = false;
      changed();
    }
  }

  Future<CommandReply?> perform(
    AppCommand command, {
    bool approval = false,
  }) async {
    if (_disposed || loading) return null;
    loading = true;
    _clearResult();
    changed();
    try {
      return await session.execute(command);
    } catch (e) {
      _fail(e, approval: approval);
      return null;
    } finally {
      loading = false;
      changed();
    }
  }

  @override
  void dispose() {
    _disposed = true;
    unawaited(_subscription?.cancel());
    unawaited(session.dispose());
    super.dispose();
  }
}

class ScreenStatus extends StatelessWidget {
  const ScreenStatus(this.controller, {super.key});
  final ScreenController controller;
  @override
  Widget build(BuildContext context) => Column(
    mainAxisSize: MainAxisSize.min,
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      if (controller.loading) const LinearProgressIndicator(),
      if (controller.error case final error?)
        Padding(
          padding: const EdgeInsets.all(12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                error,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
              TextButton(
                onPressed: controller.refresh,
                child: const Text('Refresh'),
              ),
            ],
          ),
        ),
    ],
  );
}

/// A settings screen's progress bar, shown under the app bar so it stays
/// clear of the system navigation bar.
PreferredSizeWidget settingsProgress(ScreenController state) => PreferredSize(
  preferredSize: const Size.fromHeight(4),
  child: state.loading
      ? const LinearProgressIndicator()
      : const SizedBox(height: 4),
);

Widget inputField(
  String label,
  TextEditingController controller, {
  int lines = 1,
  bool number = false,
}) => Padding(
  padding: const EdgeInsets.symmetric(vertical: 6),
  child: TextFormField(
    controller: controller,
    decoration: InputDecoration(
      labelText: label,
      border: const OutlineInputBorder(),
    ),
    maxLines: lines,
    keyboardType: number
        ? const TextInputType.numberWithOptions(decimal: true)
        : TextInputType.text,
  ),
);
