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
      startsWith(
        'MCP server "tools": HTTP 401 during initialize: unknown API key.',
      ),
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
        'Inference endpoint "local": failed during chat request: bad body. ',
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
