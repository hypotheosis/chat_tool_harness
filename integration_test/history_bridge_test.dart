import 'dart:convert';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:chat_tool_harness/backend/frb_backend.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/backend/platform_services.dart';
import 'package:chat_tool_harness/features/history/history_screen.dart';
import 'package:chat_tool_harness/src/rust/frb_generated.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() => RustLib.init());
  testWidgets(
    'real history bridge rename reopen redacted export and owned deletion',
    (tester) async {
      final dir = await Directory.systemTemp.createTemp('task13-history-');
      const oldSecret = 'old-key-quote"slash\\東京';
      const newSecret = 'new-key-line\n東京';
      final keys = <String, String>{};
      final resolved = <String>[];
      final services = PlatformServices(
        resolveCredential: (ref) async {
          resolved.add(ref);
          return keys[ref];
        },
        setServiceActive: (r) async {
          if (r.active) throw StateError('fixture refuses network admission');
        },
      );
      var b = await FrbBackend.open(dir.path, services);
      try {
        Future<String> id() async => (await b.execute(const NewId())).id!;
        final endpoint = await id(),
            profile = await id(),
            oldRef = await id(),
            newRef = await id();
        keys[oldRef] = oldSecret;
        keys[newRef] = newSecret;
        EndpointConfig config(String ref) => EndpointConfig(
          id: endpoint,
          name: 'not exported config',
          baseUrl: 'https://never-contact.invalid/v1',
          model: 'manual',
          contextTokens: 4096,
          maxOutputTokens: 128,
          temperature: 0,
          allowHttp: false,
          credential: ref,
        );
        await b.execute(SaveEndpoint(config(oldRef)));
        await b.execute(
          SaveProfile(
            Profile(
              id: profile,
              name: 'fixture',
              endpointId: endpoint,
              serverIds: const [],
              instructions: 'private system instructions',
              limits: const RunLimits(),
              mode: ProfileMode.chatOnly,
            ),
          ),
        );
        final owner = (await b.execute(CreateConversation(profile))).id!;
        final foreign = (await b.execute(CreateConversation(profile))).id!;
        await b.execute(SaveEndpoint(config(newRef)));
        await b.execute(ApplyProfile(owner, profile));
        final snapshot = await b.snapshot();
        final conversation = snapshot.conversations.firstWhere(
          (c) => c.id == owner,
        );
        await b.execute(
          AcknowledgeContextDisclosure(
            owner,
            conversation.profile.inferenceOrigin!,
          ),
        );
        final message = jsonEncode({
          'text': "literal % ' 東京",
          'old': oldSecret,
          'new': newSecret,
        });
        final runId = (await b.execute(StartTurn(owner, message))).id!;
        for (var i = 0; i < 200; i++) {
          if ((await b.snapshot()).runs
                  .firstWhere((r) => r.id == runId)
                  .state !=
              RunState.running) {
            break;
          }
          await tester.pump(const Duration(milliseconds: 10));
        }
        expect(
          (await b.snapshot()).runs.firstWhere((r) => r.id == runId).state,
          RunState.paused,
        );
        await b.execute(RenameConversation(owner, 'Durable 東京 %'));
        final owned = (await b.execute(
          CreateArtifact(owner, 'owned.txt', 'owned'),
        )).id!;
        final kept = (await b.execute(
          CreateArtifact(foreign, 'kept.txt', 'kept'),
        )).id!;
        final ownedPath = (await b.execute(
          ReadArtifact(owner, owned),
        )).artifact!.privatePath;
        await b.close();
        b = await FrbBackend.open(dir.path, services);
        final matches = (await b.execute(
          const SearchConversations("% ' 東京", limit: 0xffffffff),
        )).history;
        expect(matches.single.id, owner);
        expect(matches.single.title, 'Durable 東京 %');
        expect(
          (await b.execute(
            const SearchConversations('private system instructions'),
          )).history,
          isEmpty,
        );
        resolved.clear();
        final bundle = (await b.execute(ExportTranscript(owner))).exportBundle!;
        expect(resolved.toSet(), {oldRef, newRef});
        final text = (await b.execute(
          ReadArtifact(owner, bundle.transcript.id),
        )).artifact!.text;
        expect(text, contains('not a restorable backup'));
        expect(text, contains('[REDACTED]'));
        for (final excluded in [
          oldSecret,
          newSecret,
          'old-key',
          'new-key',
          oldRef,
          newRef,
          dir.path,
          'private system instructions',
          'never-contact.invalid',
        ]) {
          expect(text, isNot(contains(excluded)));
        }
        keys.remove(oldRef);
        await expectLater(
          b.execute(ExportTranscript(owner)),
          throwsA(
            isA<BackendFailure>().having(
              (e) => e.code,
              'missing historical key',
              FailureCode.credentialMissing,
            ),
          ),
        );
        expect(
          (await b.execute(
            const SearchConversations('Durable'),
          )).history.single.id,
          owner,
        );
        await tester.pumpWidget(MaterialApp(home: HistoryScreen(backend: b)));
        await tester.pumpAndSettle();
        expect(find.text('Durable 東京 %'), findsOneWidget);
        await tester.pumpWidget(const SizedBox());
        await tester.pumpAndSettle();
        await b.execute(DeleteConversation(owner, confirmedUnknown: false));
        expect(await File(ownedPath).exists(), isFalse);
        await b.close();
        b = await FrbBackend.open(dir.path, services);
        expect(
          (await b.execute(const SearchConversations('Durable'))).history,
          isEmpty,
        );
        expect(
          (await b.execute(ReadArtifact(foreign, kept))).artifact!.text,
          'kept',
        );
        expect((await b.snapshot()).profiles.single.id, profile);
        expect((await b.execute(CreateConversation(profile))).id, isNotNull);
      } finally {
        await b.close();
        await dir.delete(recursive: true);
      }
    },
  );
  testWidgets(
    'real bridge archive keeps history and refuses turns until restored',
    (tester) async {
      final dir = await Directory.systemTemp.createTemp('issue8-archive-');
      final services = PlatformServices(
        resolveCredential: (_) async => null,
        setServiceActive: (r) async {
          if (r.active) throw StateError('fixture refuses network admission');
        },
      );
      var b = await FrbBackend.open(dir.path, services);
      try {
        Future<String> id() async => (await b.execute(const NewId())).id!;
        final endpoint = await id(), profile = await id();
        await b.execute(
          SaveEndpoint(
            EndpointConfig(
              id: endpoint,
              name: 'endpoint',
              baseUrl: 'https://never-contact.invalid/v1',
              model: 'manual',
              contextTokens: 4096,
              maxOutputTokens: 128,
              temperature: 0,
              allowHttp: false,
            ),
          ),
        );
        await b.execute(
          SaveProfile(
            Profile(
              id: profile,
              name: 'fixture',
              endpointId: endpoint,
              serverIds: const [],
              instructions: '',
              limits: const RunLimits(),
              mode: ProfileMode.chatOnly,
            ),
          ),
        );
        final owner = (await b.execute(CreateConversation(profile))).id!;
        await b.execute(RenameConversation(owner, 'Archived 東京'));
        final file = (await b.execute(
          CreateArtifact(owner, 'kept.txt', 'kept'),
        )).id!;
        await b.execute(ArchiveConversation(owner));
        await b.close();
        // The archived flag survives a reopen, across snapshot and search.
        b = await FrbBackend.open(dir.path, services);
        expect(
          (await b.snapshot()).conversations
              .singleWhere((c) => c.id == owner)
              .archived,
          isTrue,
        );
        final found = (await b.execute(
          const SearchConversations('東京'),
        )).history.single;
        expect((found.id, found.archived), (owner, true));
        expect(
          (await b.execute(
            const SearchConversations('', archivedOnly: true),
          )).history.single.id,
          owner,
        );
        expect(
          (await b.execute(ReadArtifact(owner, file))).artifact!.text,
          'kept',
        );
        await expectLater(
          b.execute(StartTurn(owner, 'refused')),
          throwsA(
            isA<BackendFailure>().having(
              (e) => e.code,
              'archived',
              FailureCode.conflict,
            ),
          ),
        );
        await b.execute(RestoreConversation(owner));
        expect(
          (await b.execute(
            const SearchConversations('', archivedOnly: true),
          )).history,
          isEmpty,
        );
        expect(
          (await b.snapshot()).conversations
              .singleWhere((c) => c.id == owner)
              .archived,
          isFalse,
        );
      } finally {
        await b.close();
        await dir.delete(recursive: true);
      }
    },
  );
}
