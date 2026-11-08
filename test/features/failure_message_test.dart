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
}
