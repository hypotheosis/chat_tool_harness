import 'dart:async';
import 'package:flutter/material.dart';
import '../backend/backend.dart';
import '../backend/models.dart';

String failureMessage(Object error, {bool approval = false}) {
  final code = error is BackendFailure
      ? error.code
      : FailureCode.unavailableTransport;
  return switch (code) {
    FailureCode.authentication =>
      'Authentication failed or credential missing. Repair the credential reference in Settings; secure token entry arrives in Task 12.',
    FailureCode.credentialMissing =>
      'The stored credential is missing. Re-enter it in settings.',
    FailureCode.conflict when approval =>
      'This is a stale approval or changed policy. Refresh and review the current exact arguments.',
    FailureCode.conflict =>
      'Service busy or state changed. No work was queued. Refresh before trying again.',
    FailureCode.invalidInput =>
      'Invalid configuration or input. Check required fields and numeric limits.',
    FailureCode.configuration =>
      'Configuration needs repair. Open Settings and check the endpoint, profile and server linkage.',
    FailureCode.toolUnsupported =>
      'Unsupported tool calling. Explicitly choose Chat only in the profile if desired.',
    FailureCode.discoveryUnavailable =>
      'Discovery unavailable. Enter the model ID manually or check MCP transport/version.',
    FailureCode.contextLength =>
      'Context too large. Choose context explicitly or request a summary.',
    FailureCode.rateLimited =>
      'Provider rate limited this request. Nothing was automatically retried.',
    FailureCode.cancelled =>
      'Operation stopped locally. Remote effects are not rolled back.',
    FailureCode.limit => 'A configured safety limit was reached.',
    FailureCode.storage =>
      'Private storage unavailable. Refresh before continuing.',
    FailureCode.protocol =>
      'Unexpected protocol response. Check configuration; response details are withheld.',
    FailureCode.unavailableTransport =>
      'Service unavailable. Check connectivity and configuration.',
  };
}

class ScreenController extends ChangeNotifier {
  ScreenController(Backend backend) : session = BackendSession(backend);
  final BackendSession session;
  AppSnapshot? get snapshot => session.current;
  bool loading = false;
  String? error;
  bool _disposed = false;
  StreamSubscription<AppSnapshot>? _subscription;
  void changed() {
    if (!_disposed) notifyListeners();
  }

  Future<void> connect() async {
    _subscription = session.changes.listen(
      (_) => changed(),
      onError: (Object e) {
        error = failureMessage(e);
        changed();
      },
    );
    loading = true;
    changed();
    try {
      await session.connect();
    } catch (e) {
      error = failureMessage(e);
    } finally {
      loading = false;
      changed();
    }
  }

  Future<void> refresh() async {
    if (_disposed || loading) return;
    error = null;
    loading = true;
    changed();
    try {
      await session.connect();
    } catch (e) {
      error = failureMessage(e);
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
    error = null;
    changed();
    try {
      return await session.execute(command);
    } catch (e) {
      error = failureMessage(e, approval: approval);
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
