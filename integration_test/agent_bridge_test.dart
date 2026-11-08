import 'dart:async';
import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:chat_tool_harness/backend/frb_backend.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/backend/platform_services.dart';
import 'package:chat_tool_harness/src/rust/frb_generated.dart';
import 'package:chat_tool_harness/src/rust/api/agent.dart' as native;
import 'package:chat_tool_harness/src/rust/application.dart' as callbacks;
import 'package:chat_tool_harness/src/rust/model.dart' as platform;

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() => RustLib.init());
  testWidgets('chat_only_mode_and_diagnostics_use_real_generated_mappings', (
    tester,
  ) async {
    final dir = await Directory.systemTemp.createTemp('agent-ui-bridge-');
    final backend = await FrbBackend.open(
      dir.path,
      PlatformServices(
        resolveCredential: (_) async => null,
        setServiceActive: (_) async {},
      ),
    );
    try {
      final endpoint = (await backend.execute(const NewId())).id!;
      final server = (await backend.execute(const NewId())).id!;
      final profile = (await backend.execute(const NewId())).id!;
      final credential = (await backend.execute(const NewId())).id!;
      await backend.execute(
        SaveEndpoint(
          EndpointConfig(
            id: endpoint,
            name: 'endpoint',
            baseUrl: 'https://never-contact.invalid/v1',
            model: 'manual',
            contextTokens: 4000,
            maxOutputTokens: 128,
            temperature: 0,
            allowHttp: false,
            credential: credential,
          ),
        ),
      );
      await backend.execute(
        SaveServer(
          ServerConfig(
            id: server,
            name: 'server',
            url: 'https://never-contact.invalid/mcp',
            allowHttp: false,
            secretHeaders: [
              SecretHeader(name: 'Authorization', credential: credential),
            ],
          ),
        ),
      );
      await backend.execute(
        SaveProfile(
          Profile(
            id: profile,
            name: 'profile',
            endpointId: endpoint,
            serverIds: [server],
            instructions: '',
            limits: const RunLimits(),
            mode: ProfileMode.chatOnly,
          ),
        ),
      );
      final conversation = (await backend.execute(
        CreateConversation(profile),
      )).id!;
      final snapshot = await backend.snapshot();
      expect(snapshot.profiles.single.mode, ProfileMode.chatOnly);
      expect(snapshot.conversations.single.profile.mode, ProfileMode.chatOnly);
      expect(
        snapshot.conversations.single.profile.inferenceOrigin,
        'https://never-contact.invalid',
      );
      expect(snapshot.conversations.single.originAcknowledged, isFalse);
      await backend.execute(
        AcknowledgeContextDisclosure(
          conversation,
          snapshot.conversations.single.profile.inferenceOrigin!,
        ),
      );
      expect(
        (await backend.snapshot()).conversations.single.originAcknowledged,
        isTrue,
      );
      // Inference diagnostics report the missing credential itself; MCP
      // diagnostics still collapse it into authentication.
      for (final (command, code) in <(AppCommand, FailureCode)>[
        (DiscoverModels(endpoint), FailureCode.credentialMissing),
        (ProbeEndpoint(endpoint), FailureCode.credentialMissing),
        (DiscoverServer(profile, server), FailureCode.authentication),
      ]) {
        await expectLater(
          backend.execute(command),
          throwsA(
            isA<BackendFailure>().having(
              (e) => e.code,
              'missing credential is a credential failure, not unsupported',
              code,
            ),
          ),
        );
      }
      expect(
        (await backend.snapshot()).profiles.single.mode,
        ProfileMode.chatOnly,
      );
    } finally {
      await backend.close();
      await dir.delete(recursive: true);
    }
  });
  testWidgets('numeric_boundaries_reject_before_encoding_without_persistence', (
    tester,
  ) async {
    final dir = await Directory.systemTemp.createTemp('agent-numbers-');
    final services = PlatformServices(
      resolveCredential: (_) async => null,
      setServiceActive: (_) async {},
    );
    var backend = await FrbBackend.open(dir.path, services);
    const endpointId = '11111111-1111-4111-8111-111111111111';
    const profileId = '22222222-2222-4222-8222-222222222222';
    EndpointConfig endpoint(int context, int output, {String name = 'valid'}) =>
        EndpointConfig(
          id: endpointId,
          name: name,
          baseUrl: 'https://example.invalid/v1',
          model: 'test',
          contextTokens: context,
          maxOutputTokens: output,
          temperature: 0,
          allowHttp: false,
        );
    Profile profile(
      int requests,
      int tools,
      int timeout, {
      String name = 'valid',
    }) => Profile(
      id: profileId,
      name: name,
      endpointId: endpointId,
      serverIds: const [],
      instructions: 'unchanged',
      limits: RunLimits(
        maxInferenceRequests: requests,
        maxToolAttempts: tools,
        activeTimeoutMs: timeout,
      ),
    );
    try {
      const u32Max = 0xffffffff;
      const signedMax = 0x7fffffffffffffff;
      await backend.execute(SaveEndpoint(endpoint(u32Max, u32Max)));
      await backend.execute(SaveProfile(profile(u32Max, u32Max, signedMax)));
      for (final value in [-1, 0x100000000, 0x100000001]) {
        for (final command in <AppCommand>[
          SaveEndpoint(endpoint(value, 1, name: 'must not persist')),
          SaveEndpoint(endpoint(1, value, name: 'must not persist')),
          SaveProfile(profile(value, 1, 1, name: 'must not persist')),
          SaveProfile(profile(1, value, 1, name: 'must not persist')),
        ]) {
          await expectLater(
            backend.execute(command),
            throwsA(
              isA<BackendFailure>().having(
                (e) => e.code,
                'numeric boundary $value',
                FailureCode.invalidInput,
              ),
            ),
          );
          final snapshot = await backend.snapshot();
          expect(snapshot.endpoints.single.name, 'valid');
          expect(snapshot.profiles.single.name, 'valid');
        }
      }
      await expectLater(
        backend.execute(
          SaveProfile(profile(1, 1, -1, name: 'must not persist')),
        ),
        throwsA(
          isA<BackendFailure>().having(
            (e) => e.code,
            'unsigned timeout',
            FailureCode.invalidInput,
          ),
        ),
      );
      await backend.close();
      backend = await FrbBackend.open(dir.path, services);
      final saved = await backend.snapshot();
      expect(saved.endpoints.single.contextTokens, u32Max);
      expect(saved.endpoints.single.maxOutputTokens, u32Max);
      expect(saved.profiles.single.limits.maxInferenceRequests, u32Max);
      expect(saved.profiles.single.limits.maxToolAttempts, u32Max);
      expect(saved.profiles.single.limits.activeTimeoutMs, signedMax);
      expect(saved.profiles.single.name, 'valid');
    } finally {
      await backend.close();
      await dir.delete(recursive: true);
    }
  });
  testWidgets('empty_native_replay_releases_subscription', (tester) async {
    final dir = await Directory.systemTemp.createTemp('agent-empty-');
    final handle = await native.openAgent(
      dataDir: dir.path,
      resolveCredential: (_) async => const callbacks.CredentialReply.missing(),
      setServiceActive: (_) async => const platform.PlatformReply.ok(),
    );
    try {
      final events = await native
          .agentEvents(handle: handle, afterSequence: BigInt.zero)
          .toList()
          .timeout(const Duration(seconds: 10));
      expect(events, isEmpty);
    } finally {
      await native.closeAgent(handle: handle);
      await dir.delete(recursive: true);
    }
  });
  testWidgets('real_service_and_credential_failure_are_sanitized', (
    tester,
  ) async {
    final dir = await Directory.systemTemp.createTemp('agent-callback-');
    var resolutions = 0;
    final hostCalls = <bool>[];
    final android = PlatformServices.android();
    final services = PlatformServices(
      resolveCredential: (_) async {
        resolutions++;
        throw StateError('private-token/path');
      },
      setServiceActive: (active) async {
        hostCalls.add(active.active);
        await android.setServiceActive(active);
      },
    );
    final backend = await FrbBackend.open(dir.path, services);
    try {
      final identifier = (await backend.execute(const NewId())).id!;
      expect(
        identifier,
        matches(
          RegExp(
            r'^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$',
          ),
        ),
      );
      const endpoint = EndpointConfig(
        id: '11111111-1111-4111-8111-111111111111',
        name: 'no network',
        baseUrl: 'https://example.invalid/v1',
        model: 'test',
        contextTokens: 4096,
        maxOutputTokens: 128,
        temperature: 0,
        allowHttp: false,
        credential: '33333333-3333-4333-8333-333333333333',
      );
      const profile = Profile(
        id: '22222222-2222-4222-8222-222222222222',
        name: 'callback',
        endpointId: '11111111-1111-4111-8111-111111111111',
        serverIds: [],
        instructions: 'test',
        limits: RunLimits(),
      );
      await backend.execute(const SaveEndpoint(endpoint));
      await backend.execute(const SaveProfile(profile));
      final conversation = (await backend.execute(
        const CreateConversation('22222222-2222-4222-8222-222222222222'),
      )).id!;
      await backend.execute(StartTurn(conversation, 'never transmitted'));
      final terminal = await backend
          .events(0)
          .asyncMap((_) => backend.snapshot())
          .firstWhere((s) => s.runs.single.state == RunState.failed)
          .timeout(const Duration(seconds: 15));
      expect(resolutions, 1);
      expect(hostCalls, [true, false]);
      expect(terminal.runs.single.failure, 'core authentication failure');
      expect(terminal.runs.single.failure, isNot(contains('private')));
    } finally {
      await backend.close();
      await dir.delete(recursive: true);
    }
  });
  testWidgets('reopen_and_event_reconnect', (tester) async {
    final dir = await Directory.systemTemp.createTemp('agent-bridge-');
    final startup = Completer<void>();
    final services = PlatformServices(
      resolveCredential: (_) async => null,
      setServiceActive: (active) async {
        if (active.active) await startup.future;
      },
    );
    var backend = await FrbBackend.open(dir.path, services);
    try {
      final identifier = (await backend.execute(const NewId())).id!;
      expect(
        identifier,
        matches(
          RegExp(
            r'^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$',
          ),
        ),
      );
      const endpoint = EndpointConfig(
        id: '11111111-1111-4111-8111-111111111111',
        name: 'offline',
        baseUrl: 'https://example.invalid/v1',
        model: 'test',
        contextTokens: 4096,
        maxOutputTokens: 128,
        temperature: 0,
        allowHttp: false,
      );
      const profile = Profile(
        id: '22222222-2222-4222-8222-222222222222',
        name: 'test profile',
        endpointId: '11111111-1111-4111-8111-111111111111',
        serverIds: [],
        instructions: 'original',
        limits: RunLimits(),
      );
      await backend.execute(const SaveEndpoint(endpoint));
      await backend.execute(const SaveProfile(profile));
      final conversation = (await backend.execute(
        const CreateConversation('22222222-2222-4222-8222-222222222222'),
      )).id!;
      final initial = await backend.snapshot();
      expect(initial.conversations.single.profile.instructions, 'original');
      await expectLater(
        backend.execute(const CancelRun('invalid-secret-path')),
        throwsA(
          isA<BackendFailure>().having(
            (e) => e.code,
            'code',
            FailureCode.invalidInput,
          ),
        ),
      );
      final run = (await backend.execute(
        StartTurn(conversation, 'persisted input'),
      )).id!;
      await backend.execute(CancelRun(run));
      startup.complete();
      final snapshot = await backend.snapshot();
      expect(
        snapshot.conversations.single.messages.single.content,
        'persisted input',
      );
      expect(snapshot.runs.single.state, RunState.cancelled);
      await backend.execute(
        const SaveServer(
          ServerConfig(
            id: '44444444-4444-4444-8444-444444444444',
            name: 'saved only',
            url: 'https://server.invalid/mcp',
            allowHttp: false,
            secretHeaders: [
              SecretHeader(
                name: 'X-Key',
                credential: '33333333-3333-4333-8333-333333333333',
              ),
            ],
          ),
        ),
      );
      await backend.execute(
        const SaveProfile(
          Profile(
            id: '22222222-2222-4222-8222-222222222222',
            name: 'updated profile',
            endpointId: '11111111-1111-4111-8111-111111111111',
            serverIds: [],
            instructions: 'updated',
            limits: RunLimits(),
          ),
        ),
      );
      expect(
        (await backend.snapshot()).conversations.single.profile.instructions,
        'original',
      );
      await backend.execute(ApplyProfile(conversation, profile.id));
      await backend.execute(
        AcknowledgeContextDisclosure(conversation, 'https://example.invalid'),
      );
      final updated = await backend.snapshot();
      expect(updated.conversations.single.profile.instructions, 'updated');
      expect(updated.runs.single.profile.instructions, 'original');
      expect(updated.servers.single.secretHeaders.single.name, 'X-Key');
      await expectLater(
        backend.execute(
          ApplyContextSelection(
            run,
            ContextSelection(
              messageIds: [snapshot.conversations.single.messages.single.id],
              summaryIds: const [],
            ),
          ),
        ),
        throwsA(
          isA<BackendFailure>().having(
            (e) => e.code,
            'code',
            FailureCode.conflict,
          ),
        ),
      );
      final replay = await backend
          .events(0)
          .first
          .timeout(const Duration(seconds: 10));
      expect(replay.sequence, greaterThan(0));
      final next = await backend
          .events(replay.sequence)
          .first
          .timeout(const Duration(seconds: 10));
      expect(next.sequence, greaterThan(replay.sequence));
      await backend.close();
      backend = await FrbBackend.open(dir.path, services);
      final reopened = await backend.snapshot();
      expect(reopened.conversations.single.id, conversation);
      expect(reopened.conversations.single.profile.endpoint.model, 'test');
      expect(reopened.conversations.single.profile.instructions, 'updated');
      expect(reopened.runs.single.profile.instructions, 'original');
      expect(reopened.servers.single.secretHeaders.single.name, 'X-Key');
      expect(reopened.runs.single.state, RunState.cancelled);
      expect(
        reopened.conversations.single.messages.single.content,
        'persisted input',
      );
    } finally {
      await backend.close();
      await dir.delete(recursive: true);
    }
  });
}
