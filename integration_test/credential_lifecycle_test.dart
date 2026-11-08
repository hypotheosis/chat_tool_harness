import 'dart:convert';
import 'dart:io';
import 'package:chat_tool_harness/backend/frb_backend.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/backend/platform_services.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:chat_tool_harness/main.dart' as app;

const credentials = MethodChannel('chat_tool_harness/credentials');
void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets('credential_replace_delete_invalidate', (tester) async {
    await app.main();
    await tester.pumpAndSettle();
    const reference = 'd92677ad-3f0c-4ca5-b10a-c4fcd9bf4131';
    Future<void> put(List<int> value) => credentials.invokeMethod('put', {
      'reference': reference,
      'value': Uint8List.fromList(value),
    });
    addTearDown(
      () => credentials.invokeMethod<void>('delete', {'reference': reference}),
    );
    await put([65, 66]);
    expect(
      await credentials.invokeMethod<Uint8List>('resolve', {
        'reference': reference,
      }),
      [65, 66],
    );
    await put([67]);
    expect(
      await credentials.invokeMethod<Uint8List>('resolve', {
        'reference': reference,
      }),
      [67],
    );
    await credentials.invokeMethod<void>('delete', {'reference': reference});
    // A deleted credential is absent, not a failure.
    expect(
      await credentials.invokeMethod<Uint8List>('resolve', {
        'reference': reference,
      }),
      isNull,
    );
    await expectLater(
      credentials.invokeMethod('put', {
        'reference': 'not-a-reference',
        'value': Uint8List.fromList([65]),
      }),
      throwsA(isA<PlatformException>()),
    );
  });
  testWidgets(
    'native credentials resolve JIT through the real bridge; deletion preserves history',
    (tester) async {
      final directory = await Directory.systemTemp.createTemp('task12-jit-');
      final received = <String?>[];
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      server.listen((request) async {
        received.add(request.headers.value('authorization'));
        request.response.headers.contentType = ContentType.json;
        request.response.write(
          jsonEncode({
            'data': [
              {'id': 'fixture'},
            ],
          }),
        );
        await request.response.close();
      });
      const reference = 'e92677ad-3f0c-4ca5-b10a-c4fcd9bf4131';
      const endpoint = 'f92677ad-3f0c-4ca5-b10a-c4fcd9bf4131';
      final backend = await FrbBackend.open(
        directory.path,
        PlatformServices.android(),
      );
      try {
        await backend.execute(
          SaveEndpoint(
            EndpointConfig(
              id: endpoint,
              name: 'local JIT',
              baseUrl: 'http://127.0.0.1:${server.port}',
              model: 'fixture',
              contextTokens: 4096,
              maxOutputTokens: 128,
              temperature: 0,
              allowHttp: true,
              credential: reference,
            ),
          ),
        );
        await PlatformServices.putCredential(reference, 'test-only-first');
        expect(
          await PlatformServices.android().resolveCredential(reference),
          'test-only-first',
        );
        await backend.execute(const DiscoverModels(endpoint));
        await PlatformServices.putCredential(reference, 'test-only-replaced');
        await backend.execute(const DiscoverModels(endpoint));
        expect(received, [
          'Bearer test-only-first',
          'Bearer test-only-replaced',
        ]);
        await PlatformServices.deleteCredential(reference);
        await expectLater(
          backend.execute(const DiscoverModels(endpoint)),
          throwsA(
            isA<BackendFailure>().having(
              (e) => e.code,
              'requires reentry',
              FailureCode.credentialMissing,
            ),
          ),
        );
        expect(received, hasLength(2));
        expect(
          (await backend.snapshot()).endpoints.single.credential,
          reference,
        );
        for (final file in directory.listSync().whereType<File>()) {
          final bytes = await file.readAsBytes();
          expect(latin1.decode(bytes), isNot(contains('test-only-first')));
          expect(latin1.decode(bytes), isNot(contains('test-only-replaced')));
        }
      } finally {
        await PlatformServices.deleteCredential(reference);
        await backend.close();
        await server.close(force: true);
        await directory.delete(recursive: true);
      }
    },
  );
  testWidgets('describe reports scheme and length without the secret', (
    tester,
  ) async {
    const ref = 'a92677ad-3f0c-4ca5-b10a-c4fcd9bf4131';
    addTearDown(() => PlatformServices.deleteCredential(ref));
    await PlatformServices.putCredential(ref, 'Bearer okf_example');
    final d = await PlatformServices.describeCredential(ref);
    expect(d, isNotNull);
    expect(d!.scheme, 'Bearer');
    expect(d.length, 'Bearer okf_example'.length);
    await PlatformServices.putCredential(ref, 'plainkey');
    final bare = await PlatformServices.describeCredential(ref);
    expect(bare!.scheme, isNull);
    expect(bare.length, 8);
    // Same rule as Rust's authorization_value: a trailing space or a
    // non-token first word is not a scheme.
    await PlatformServices.putCredential(ref, 'Bearer ');
    expect((await PlatformServices.describeCredential(ref))!.scheme, isNull);
    await PlatformServices.putCredential(ref, '1abc def');
    expect((await PlatformServices.describeCredential(ref))!.scheme, isNull);
    await PlatformServices.putCredential(ref, 'Token: x');
    expect((await PlatformServices.describeCredential(ref))!.scheme, isNull);
    await PlatformServices.deleteCredential(ref);
    expect(await PlatformServices.describeCredential(ref), isNull);
  });
  testWidgets('a corrupted credential is a failure, not missing', (
    tester,
  ) async {
    const ref = 'b92677ad-3f0c-4ca5-b10a-c4fcd9bf4131';
    addTearDown(() => PlatformServices.deleteCredential(ref));
    await PlatformServices.putCredential(ref, 'test-only-corrupt');
    // CredentialStore keeps ciphertext in Context.noBackupFilesDir/credentials.
    final file = File(
      '${Directory.systemTemp.parent.path}/no_backup/credentials/$ref',
    );
    expect(file.existsSync(), isTrue);
    await file.writeAsBytes(List<int>.filled(40, 7), flush: true);
    await expectLater(
      PlatformServices.android().resolveCredential(ref),
      throwsA(isA<PlatformException>()),
    );
    await expectLater(
      PlatformServices.describeCredential(ref),
      throwsA(isA<PlatformException>()),
    );
  });
}
