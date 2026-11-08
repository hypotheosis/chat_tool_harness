// Standalone test-owned APK entrypoint; driven by task12_process_recovery.py.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:chat_tool_harness/backend/backend.dart';
import 'package:chat_tool_harness/backend/engine_lifecycle.dart';
import 'package:chat_tool_harness/backend/frb_backend.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/backend/platform_services.dart';
import 'package:chat_tool_harness/src/rust/frb_generated.dart';

const _probe = MethodChannel('chat_tool_harness/agent_probe');
const _scenario = String.fromEnvironment('TASK12_SCENARIO');
const _fixture = String.fromEnvironment('TASK12_FIXTURE');
Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final path = await _probe.invokeMethod<String>(
    'scenarioDirectory',
    _scenario,
  );
  if (path == null) throw StateError('Missing test directory');
  final marker = File('$path/state.json');
  Map<String, dynamic> state = marker.existsSync()
      ? jsonDecode(marker.readAsStringSync()) as Map<String, dynamic>
      : {};
  Future<void> record(String phase) async {
    state['phase'] = phase;
    await marker.writeAsString(jsonEncode(state), flush: true);
  }

  try {
    await RustLib.init();
    final ready = Completer<Backend>();
    final lifecycle = EngineLifecycle(() => ready.future)..initialize();
    final backend = await FrbBackend.open(
      '$path/core',
      PlatformServices.android(),
    );
    ready.complete(backend);
    final native = await PlatformServices.runtime
        .invokeMapMethod<String, dynamic>('state');
    final previousRuntime = state['runtimeId'];
    state['runtimeId'] = native?['runtimeId'];
    if (previousRuntime != null && previousRuntime == state['runtimeId']) {
      throw StateError('Process did not restart');
    }
    Future<AppSnapshot> until(bool Function(AppSnapshot) condition) async {
      for (var i = 0; i < 1500; i++) {
        final snapshot = await backend.snapshot();
        if (condition(snapshot)) return snapshot;
        if (snapshot.runs.single.state != RunState.running) {
          throw StateError(
            'Unexpected ${snapshot.runs.single.state}: ${snapshot.runs.single.failure}',
          );
        }
        await Future<void>.delayed(const Duration(milliseconds: 20));
      }
      throw StateError('Device checkpoint did not arrive');
    }

    if (state['phase'] == 'dispatched') {
      final snapshot = await backend.snapshot();
      final run = snapshot.runs.single;
      if (run.state != RunState.interrupted ||
          run.attempts.single.state != AttemptState.outcomeUnknown) {
        throw StateError('Uncertain dispatch not recovered');
      }
      try {
        await backend.execute(ResumeRun(run.id));
        throw StateError('Unknown work resumed');
      } on BackendFailure catch (error) {
        if (error.code != FailureCode.conflict) rethrow;
      }
      if (native?['activeRuns'] != 0) {
        throw StateError('Service restarted automatically');
      }
      runApp(
        const MaterialApp(
          home: Scaffold(
            body: Center(child: Text('Recovered unknown; no replay')),
          ),
        ),
      );
      await lifecycle.close();
      await backend.close();
      await PlatformServices.runtime.invokeMethod<void>('stopRunService', {
        'runId': run.id,
        'conversationId': run.conversationId,
        'generation': state['generation'],
        'waitingApproval': false,
      });
      await record('passed');
      return;
    }
    if (state['phase'] == 'waiting') {
      final snapshot = await backend.snapshot();
      if (snapshot.runs.single.state != RunState.interrupted ||
          snapshot.runs.single.attempts.single.state != AttemptState.proposed ||
          native?['activeRuns'] != 0) {
        throw StateError('Approval wait not recovered idle');
      }
      final opened = Completer<void>();
      void observeOpen() {
        if (lifecycle.openConversation.value ==
                snapshot.conversations.single.id &&
            !opened.isCompleted) {
          opened.complete();
        }
      }

      lifecycle.openConversation.addListener(observeOpen);
      observeOpen();
      await opened.future.timeout(const Duration(seconds: 10));
      lifecycle.openConversation.removeListener(observeOpen);
      var used = false;
      runApp(
        MaterialApp(
          home: Scaffold(
            body: Center(
              child: FilledButton(
                onPressed: () async {
                  if (used) return;
                  used = true;
                  try {
                    await backend.execute(ResumeRun(snapshot.runs.single.id));
                    await until(
                      (s) => s.runs.single.state == RunState.waitingApproval,
                    );
                    // Await the native cleanup acknowledgment before the next explicit phase.
                    for (var i = 0; i < 100; i++) {
                      if ((await PlatformServices.runtime
                              .invokeMapMethod<String, dynamic>(
                                'state',
                              ))?['activeRuns'] ==
                          0) {
                        break;
                      }
                      await Future<void>.delayed(
                        const Duration(milliseconds: 20),
                      );
                    }
                    final binding = (await backend.snapshot())
                        .runs
                        .single
                        .attempts
                        .single
                        .binding!;
                    await backend.execute(Approve(binding));
                    await backend.execute(ResumeRun(snapshot.runs.single.id));
                    await until(
                      (s) =>
                          s.runs.single.attempts.single.state ==
                          AttemptState.dispatching,
                    );
                    state['generation'] = (await PlatformServices.runtime
                        .invokeMapMethod<String, dynamic>('state'))?['lease'];
                    await record('dispatched');
                  } catch (error) {
                    state['error'] = error.toString();
                    await record('failed');
                  }
                },
                child: const Text('Approve and resume controlled fixture'),
              ),
            ),
          ),
        ),
      );
      // A notification Open after process death is navigation only. No execution
      // occurs until the foreground test-owned button receives a real tap.
      await WidgetsBinding.instance.endOfFrame;
      await record('awaitGesture');
      return;
    }
    if (state.isNotEmpty && state['phase'] != null) {
      throw StateError('Unexpected fixture phase');
    }
    runApp(
      const MaterialApp(
        home: Scaffold(
          body: Center(child: Text('Preparing controlled fixture')),
        ),
      ),
    );
    const endpoint = '11111111-1111-4111-8111-111111111111';
    const profile = '22222222-2222-4222-8222-222222222222';
    const server = '33333333-3333-4333-8333-333333333333';
    await backend.execute(
      SaveEndpoint(
        EndpointConfig(
          id: endpoint,
          name: 'local',
          baseUrl: _fixture,
          model: 'fixture',
          contextTokens: 32000,
          maxOutputTokens: 256,
          temperature: 0,
          allowHttp: true,
        ),
      ),
    );
    await backend.execute(
      SaveServer(
        ServerConfig(
          id: server,
          name: 'controlled fixture',
          url: '$_fixture/mcp',
          allowHttp: true,
        ),
      ),
    );
    await backend.execute(
      const SaveProfile(
        Profile(
          id: profile,
          name: 'fixture',
          endpointId: endpoint,
          serverIds: [server],
          instructions: '',
          limits: RunLimits(),
        ),
      ),
    );
    final conversation = (await backend.execute(
      const CreateConversation(profile),
    )).id!;
    final saved = (await backend.snapshot()).conversations.single.profile;
    await backend.execute(
      AcknowledgeContextDisclosure(conversation, saved.inferenceOrigin!),
    );
    final run = (await backend.execute(
      StartTurn(conversation, 'controlled test write'),
    )).id!;
    await until((s) => s.runs.single.state == RunState.waitingApproval);
    // Core persists the wait before native cleanup; wait for the real stop ack.
    for (var i = 0; i < 100; i++) {
      final state = await PlatformServices.runtime
          .invokeMapMethod<String, dynamic>('state');
      if (state?['activeRuns'] == 0) break;
      await Future<void>.delayed(const Duration(milliseconds: 20));
    }
    if ((await PlatformServices.runtime.invokeMapMethod<String, dynamic>(
          'state',
        ))?['activeRuns'] !=
        0) {
      throw StateError('Wait service still active');
    }
    state['runId'] = run;
    state['conversationId'] = conversation;
    await record('waiting');
  } catch (error) {
    state['error'] = error.toString();
    await record('failed');
    runApp(const MaterialApp(home: Scaffold(body: Text('Fixture failed'))));
  }
}
