import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/screen_controller.dart';

void main() {
  test('credential missing has its own message', () {
    final missing = failureMessage(
      const BackendFailure(FailureCode.credentialMissing),
    );
    expect(missing, contains('credential is missing'));
    expect(
      missing,
      isNot(failureMessage(const BackendFailure(FailureCode.authentication))),
    );
  });

  test('a run limit reads as its reason and how to raise it', () {
    const failure = BackendFailure(
      FailureCode.limit,
      detail: FailureDetail(
        targetKind: 'profile',
        targetName: 'Default',
        step: 'runLimit',
        reason:
            "Run time limit (15 min) reached; time waiting for the model's first output is not counted",
      ),
    );
    expect(
      failureMessage(failure),
      "Run time limit (15 min) reached; time waiting for the model's first "
      "output is not counted. Raise it in the profile's limits if the run "
      'needs more.',
    );
  });
  test('a stream size cap names the endpoint and the cap', () {
    const failure = BackendFailure(
      FailureCode.limit,
      detail: FailureDetail(
        targetKind: 'inference',
        targetName: 'local',
        step: 'chatRequest',
        reason: 'the reply exceeded the reply text limit (1 MiB)',
      ),
    );
    expect(
      failureMessage(failure),
      'Inference endpoint "local": failed during the chat request: the reply '
      'exceeded the reply text limit (1 MiB). A configured safety limit was '
      'reached.',
    );
  });
  test('detail names the target, status, step and reason', () {
    const failure = BackendFailure(
      FailureCode.authentication,
      detail: FailureDetail(
        targetKind: 'mcpServer',
        targetName: 'tools',
        step: 'initialize',
        status: 401,
        reason: 'unknown API key',
      ),
    );
    expect(
      failureMessage(failure),
      'MCP server "tools": HTTP 401 during initialization: unknown API key. '
      'Check or re-enter the credential in settings.',
    );
    expect(failureMessage(failure), isNot(contains('Task 12')));
  });
  test('detail without name, status or reason still reads cleanly', () {
    const failure = BackendFailure(
      FailureCode.unavailableTransport,
      detail: FailureDetail(targetKind: 'inference', step: 'discoverModels'),
    );
    expect(
      failureMessage(failure),
      startsWith('Inference endpoint: failed during model discovery.'),
    );
    const trailing = BackendFailure(
      FailureCode.protocol,
      detail: FailureDetail(
        targetKind: 'inference',
        targetName: 'local',
        step: 'chatRequest',
        reason: 'bad body.',
      ),
    );
    expect(
      failureMessage(trailing),
      startsWith(
        'Inference endpoint "local": failed during the chat request: bad body. ',
      ),
    );
  });
  test('no code mentions a future task', () {
    for (final code in FailureCode.values) {
      expect(failureMessage(BackendFailure(code)), isNot(contains('Task')));
    }
  });
  test('each code maps to one action', () {
    expect(
      failureAction(FailureCode.credentialMissing),
      FailureAction.reenterCredential,
    );
    for (final code in [
      FailureCode.authentication,
      FailureCode.unavailableTransport,
      FailureCode.protocol,
      FailureCode.configuration,
    ]) {
      expect(failureAction(code), FailureAction.openSettings);
    }
    expect(
      failureAction(FailureCode.contextLength),
      FailureAction.chooseContext,
    );
    expect(failureAction(FailureCode.rateLimited), FailureAction.retry);
    expect(failureAction(FailureCode.storage), FailureAction.retry);
  });
}
