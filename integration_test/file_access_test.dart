import 'dart:io';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:chat_tool_harness/backend/frb_backend.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/backend/platform_services.dart';
import 'package:chat_tool_harness/features/files/file_controller.dart';
import 'package:chat_tool_harness/features/files/android_document_access.dart';
import 'package:chat_tool_harness/src/rust/frb_generated.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() => RustLib.init());
  testWidgets(
    'generated text bridge ownership restart and restricted native URI',
    (tester) async {
      final dir = await Directory.systemTemp.createTemp('task10-files-');
      final services = PlatformServices(
        resolveCredential: (_) async => null,
        setServiceActive: (_) async {},
      );
      var backend = await FrbBackend.open(dir.path, services);
      File? uriFixture;
      try {
        Future<String> newId() async =>
            (await backend.execute(const NewId())).id!;
        final endpoint = await newId();
        final profile = await newId();
        await backend.execute(
          SaveEndpoint(
            EndpointConfig(
              id: endpoint,
              name: 'fixture',
              baseUrl: 'https://example.invalid/v1',
              model: 'm',
              contextTokens: 4096,
              maxOutputTokens: 128,
              temperature: 0,
              allowHttp: false,
            ),
          ),
        );
        await backend.execute(
          SaveProfile(
            Profile(
              id: profile,
              name: 'fixture',
              endpointId: endpoint,
              serverIds: const [],
              instructions: '',
              limits: const RunLimits(
                maxInferenceRequests: 20,
                maxToolAttempts: 40,
                activeTimeoutMs: 900000,
              ),
            ),
          ),
        );
        final owner = (await backend.execute(CreateConversation(profile))).id!;
        final foreign = (await backend.execute(
          CreateConversation(profile),
        )).id!;
        final id = (await backend.execute(
          CreateArtifact(
            owner,
            'answer.md',
            '# literal <script> é ![](https://invalid/image)',
          ),
        )).id!;
        final files = FileController(
          backend,
          const MethodChannelDocumentAccess(),
          owner,
        );
        expect(
          await files.preview(id),
          '# literal <script> é ![](https://invalid/image)',
        );
        final artifact = (await backend.execute(
          ReadArtifact(owner, id),
        )).artifact!;
        expect(artifact.summary.bytes, File(artifact.privatePath).lengthSync());
        expect(artifact.privatePath, endsWith('$id.txt'));
        expect((await backend.snapshot()).retentionLimitBytes, 268435456);
        for (final command in <AppCommand>[
          ReadArtifact(foreign, id),
          DeleteArtifact(foreign, id),
          CreateArtifact(owner, '../escape.md', 'x'),
        ]) {
          await expectLater(
            backend.execute(command),
            throwsA(isA<BackendFailure>()),
          );
        }
        await backend.close();
        backend = await FrbBackend.open(dir.path, services);
        expect((await backend.snapshot()).artifacts.single.id, id);
        expect(
          (await backend.execute(ReadArtifact(owner, id))).artifact!.text,
          artifact.text,
        );
        const documents = MethodChannelDocumentAccess.channel;
        await expectLater(
          documents.invokeMethod<String>('prepareArtifact', {
            'privatePath': artifact.privatePath,
            'name': 'answer.md',
          }),
          throwsA(isA<PlatformException>()),
        );
        // This disposable private fixture exercises the production provider root/channel without opening a chooser.
        final root = Directory(
          '${await PlatformServices.applicationDataDir()}/artifacts',
        );
        await root.create(recursive: true);
        uriFixture = File('${root.path}/${await newId()}.txt');
        await uriFixture.writeAsString(artifact.text);
        final uri = Uri.parse(
          (await documents.invokeMethod<String>('prepareArtifact', {
            'privatePath': uriFixture.path,
            'name': 'answer.md',
          }))!,
        );
        expect(uri.scheme, 'content');
        expect(uri.authority, 'com.example.chat_tool_harness.artifacts');
        expect(uri.path, contains('/generated_text/'));
        await backend.execute(DeleteArtifact(owner, id));
        expect((await backend.snapshot()).artifacts, isEmpty);
        expect(await File(artifact.privatePath).exists(), isFalse);
      } finally {
        await backend.close();
        if (uriFixture != null && await uriFixture.exists()) {
          await uriFixture.delete();
        }
        await dir.delete(recursive: true);
      }
    },
  );
}
