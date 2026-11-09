import 'dart:convert';
import 'dart:io';
import 'package:chat_tool_harness/backend/frb_backend.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/backend/platform_services.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:chat_tool_harness/main.dart' as app;
import 'authorization_parity_cases.dart';

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
  testWidgets('calls issued together run in arrival order', (tester) async {
    const ref = 'c92677ad-3f0c-4ca5-b10a-c4fcd9bf4131';
    addTearDown(() => PlatformServices.deleteCredential(ref));
    Future<Uint8List?> resolve() =>
        credentials.invokeMethod<Uint8List>('resolve', {'reference': ref});
    // Nothing is awaited between calls: each must see the one before it.
    final results = await Future.wait<Object?>([
      PlatformServices.putCredential(ref, 'first'),
      resolve(),
      PlatformServices.putCredential(ref, 'second'),
      resolve(),
      PlatformServices.deleteCredential(ref),
      resolve(),
    ]);
    expect(results[1], utf8.encode('first'));
    expect(results[3], utf8.encode('second'));
    expect(results[5], isNull);
  });
  testWidgets('describe reports scheme and length without the secret', (
    tester,
  ) async {
    const ref = 'a92677ad-3f0c-4ca5-b10a-c4fcd9bf4131';
    addTearDown(() => PlatformServices.deleteCredential(ref));
    // Same table Rust's net_authorization_value_rule checks.
    for (final c in authorizationParityCases) {
      await PlatformServices.putCredential(ref, c.value);
      final d = await PlatformServices.describeCredential(ref);
      final label = jsonEncode(c.value);
      expect(d, isNotNull, reason: label);
      expect(d!.scheme, c.scheme, reason: label);
      expect(d.length, utf8.encode(c.value).length, reason: label);
      expect(d.sendable, c.sendable, reason: label);
    }
    await PlatformServices.deleteCredential(ref);
    expect(await PlatformServices.describeCredential(ref), isNull);
  });
  testWidgets(
    'a corrupted or unreadable credential is a failure, not missing',
    (tester) async {
      const ref = 'b92677ad-3f0c-4ca5-b10a-c4fcd9bf4131';
      addTearDown(() => PlatformServices.deleteCredential(ref));
      // CredentialStore keeps ciphertext in Context.noBackupFilesDir/credentials.
      // On stock Android, Flutter's Directory.systemTemp is Context.cacheDir
      // (<dataDir>/cache) and noBackupFilesDir is <dataDir>/no_backup, so the
      // file sits at systemTemp.parent/no_backup (applicationDataDir relies on
      // the same layout). The existsSync check fails loudly if a device differs.
      final file = File(
        '${Directory.systemTemp.parent.path}/no_backup/credentials/$ref',
      );
      Future<void> expectFailure(String why) async {
        await expectLater(
          PlatformServices.android().resolveCredential(ref),
          throwsA(isA<PlatformException>()),
          reason: why,
        );
        await expectLater(
          PlatformServices.describeCredential(ref),
          throwsA(
            isA<BackendFailure>().having(
              (e) => e.code,
              'code',
              FailureCode.authentication,
            ),
          ),
          reason: why,
        );
      }

      await PlatformServices.putCredential(ref, 'test-only-corrupt');
      expect(file.existsSync(), isTrue);
      // Right size, wrong bytes: GCM authentication fails.
      await file.writeAsBytes(List<int>.filled(40, 7), flush: true);
      await expectFailure('garbage ciphertext');
      // Shorter than IV + tag + 1 byte (29): the size check fails.
      await file.writeAsBytes(List<int>.filled(10, 7), flush: true);
      await expectFailure('truncated file');
      // Present but unreadable: FileNotFoundException must not mean missing.
      await PlatformServices.putCredential(ref, 'test-only-unreadable');
      final chmod = await Process.run('chmod', ['000', file.path]);
      expect(chmod.exitCode, 0, reason: '${chmod.stderr}');
      await expectFailure('unreadable file');
      // It corrupts Android's ciphertext files; Linux keeps credentials in the
      // Secret Service, which this test cannot corrupt.
    },
    skip: !Platform.isAndroid,
  );
}
