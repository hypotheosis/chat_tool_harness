import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/backend/platform_services.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  const ref = 'a92677ad-3f0c-4ca5-b10a-c4fcd9bf4131';
  void reply(Object? Function() answer) => messenger.setMockMethodCallHandler(
    PlatformServices.credentials,
    (call) async {
      expect(call.method, 'describe');
      expect(call.arguments, {'reference': ref});
      return answer();
    },
  );
  tearDown(
    () =>
        messenger.setMockMethodCallHandler(PlatformServices.credentials, null),
  );

  test('preview shows the secret length and whether Bearer is added', () {
    expect(
      const CredentialSummary(
        scheme: 'Bearer',
        length: 47,
        sendable: true,
      ).authorizationPreview,
      'Authorization: Bearer ••• (40 chars)',
    );
    expect(
      const CredentialSummary(length: 40, sendable: true).authorizationPreview,
      'Authorization: Bearer ••• (40 chars; Bearer added)',
    );
    expect(
      const CredentialSummary(length: 41, sendable: false).authorizationPreview,
      'Authorization: Bearer ••• (41 chars; Bearer added). '
      'Has leading/trailing whitespace or characters that cannot be sent',
    );
  });

  test('describe maps the reply, absence, and failures', () async {
    reply(() => {'scheme': 'Basic', 'length': 12, 'sendable': false});
    final summary = await PlatformServices.describeCredential(ref);
    expect(summary!.scheme, 'Basic');
    expect(summary.length, 12);
    expect(summary.sendable, isFalse);

    reply(() => null);
    expect(await PlatformServices.describeCredential(ref), isNull);

    for (final malformed in <Object>[
      'not a map',
      [1, 2],
      {'scheme': null, 'length': 3},
      {'scheme': 1, 'length': 3, 'sendable': true},
      {'scheme': null, 'length': '3', 'sendable': true},
    ]) {
      reply(() => malformed);
      await expectLater(
        PlatformServices.describeCredential(ref),
        throwsA(
          isA<BackendFailure>().having(
            (e) => e.code,
            'code',
            FailureCode.protocol,
          ),
        ),
      );
    }

    reply(() => throw PlatformException(code: 'CREDENTIAL_UNAVAILABLE'));
    await expectLater(
      PlatformServices.describeCredential(ref),
      throwsA(
        isA<BackendFailure>().having(
          (e) => e.code,
          'code',
          FailureCode.authentication,
        ),
      ),
    );
  });
}
