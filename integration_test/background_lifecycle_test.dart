import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:chat_tool_harness/app.dart';
import 'package:chat_tool_harness/backend/backend.dart';
import 'package:chat_tool_harness/backend/frb_backend.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/backend/platform_services.dart';
import 'package:chat_tool_harness/backend/engine_lifecycle.dart';
import 'package:chat_tool_harness/src/rust/frb_generated.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

const runtime = MethodChannel('chat_tool_harness/agent_runtime');
const probe = MethodChannel('chat_tool_harness/agent_probe');
const identity = {
  'generation': 'c92677ad-3f0c-4ca5-b10a-c4fcd9bf4131',
  'runId': 'a92677ad-3f0c-4ca5-b10a-c4fcd9bf4131',
  'conversationId': 'b92677ad-3f0c-4ca5-b10a-c4fcd9bf4131',
};
Future<AppSnapshot> until(
  Backend backend,
  bool Function(AppSnapshot) condition,
) async {
  final current = await backend.snapshot();
  if (condition(current)) return current;
  return backend
      .events(current.sequence)
      .asyncMap((_) => backend.snapshot())
      .map((snapshot) {
        if (!condition(snapshot) &&
            snapshot.runs.isNotEmpty &&
            snapshot.runs.every((r) => r.state != RunState.running)) {
          throw StateError(
            'Unexpected state ${snapshot.runs.last.state}: ${snapshot.runs.last.failure}; context ${snapshot.runs.last.context.usage?.reason}',
          );
        }
        return snapshot;
      })
      .firstWhere(condition)
      .timeout(const Duration(seconds: 20));
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() => RustLib.init());
  testWidgets('normal cleanup must not emit user Stop', (tester) async {
    final events = <dynamic>[];
    final sub = EngineLifecycle.events.listen(events.add);
    await runtime.invokeMethod<void>('startRunService', identity);
    await runtime.invokeMethod<void>('stopRunService', identity);
    await tester.pump(const Duration(milliseconds: 100));
    expect(events, isEmpty, reason: 'Core cleanup is not a user cancellation');
    await sub.cancel();
  });
  testWidgets(
    'pending_approval_stops_service; activity_reconnect_same_agent_handle; native_host_loss_without_replay',
    (tester) async {
      final dir = await Directory.systemTemp.createTemp('task12-background-');
      final fixture = await _LocalWriteFixture.open();
      final backend = await FrbBackend.open(
        dir.path,
        PlatformServices.android(),
      );
      final binding = EngineLifecycle(() async => backend)..initialize();
      await tester.pumpWidget(AgentApp(backend: backend, lifecycle: binding));
      await tester.pumpAndSettle();
      final initial = await runtime.invokeMapMethod<String, dynamic>('state');
      try {
        const e = '11111111-1111-4111-8111-111111111111';
        const p = '22222222-2222-4222-8222-222222222222';
        const s = '33333333-3333-4333-8333-333333333333';
        await backend.execute(
          SaveEndpoint(
            EndpointConfig(
              id: e,
              name: 'local',
              baseUrl: fixture.url,
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
              id: s,
              name: 'controlled local write',
              url: '${fixture.url}/mcp',
              allowHttp: true,
            ),
          ),
        );
        await backend.execute(
          const SaveProfile(
            Profile(
              id: p,
              name: 'local',
              endpointId: e,
              serverIds: [s],
              instructions: '',
              limits: RunLimits(),
            ),
          ),
        );
        final conversation = (await backend.execute(
          const CreateConversation(p),
        )).id!;
        final snapshot = await backend.snapshot();
        final profile = snapshot.conversations.single.profile;
        await backend.execute(
          AcknowledgeContextDisclosure(conversation, profile.inferenceOrigin!),
        );
        final run = (await backend.execute(
          StartTurn(conversation, 'test-owned controlled write'),
        )).id!;
        final waiting = await until(
          backend,
          (s) => s.runs.single.state == RunState.waitingApproval,
        );
        // Wait for native cleanup acknowledgment by taking a bounded snapshot of OS state.
        for (var i = 0; i < 100; i++) {
          if ((await runtime.invokeMapMethod<String, dynamic>(
                'state',
              ))?['activeRuns'] ==
              0) {
            break;
          }
          await tester.pump(const Duration(milliseconds: 20));
        }
        expect(
          (await runtime.invokeMapMethod<String, dynamic>(
            'state',
          ))?['activeRuns'],
          0,
        );
        expect(fixture.writes, 0);
        if (initial?['notificationVisibility'] == 'normal') {
          final notifications = await probe.invokeListMethod<dynamic>(
            'notificationState',
          );
          final pending = (notifications ?? []).cast<Map>().singleWhere(
            (n) => n['id'] == 1202,
          );
          expect(pending['actions'], 0);
          expect(pending['text'], 'Open the conversation to review');
          await probe.invokeMethod<void>('openPending');
          await tester.pumpAndSettle();
          expect(
            (await backend.snapshot()).runs.single.state,
            RunState.waitingApproval,
          );
          expect(fixture.writes, 0);
        }
        final approval = waiting.runs.single.attempts.single.binding!;
        fixture.changedSchema = true;
        await expectLater(
          backend.execute(Approve(approval)),
          throwsA(isA<BackendFailure>()),
        );
        expect(fixture.writes, 0);
        fixture.changedSchema = false;
        await backend.execute(Approve(approval));
        await backend.execute(ResumeRun(run));
        await fixture.written.future.timeout(const Duration(seconds: 20));
        expect(fixture.writes, 1);
        expect(await probe.invokeMethod<bool>('captureStop'), true);
        await probe.invokeMethod<void>('finishActivity');
        expect((await backend.snapshot()).runs.single.state, RunState.running);
        await probe.invokeMethod<void>('restoreActivity');
        final restored = await runtime.invokeMapMethod<String, dynamic>(
          'state',
        );
        expect(restored?['runtimeId'], initial?['runtimeId']);
        expect((await backend.snapshot()).runs.single.id, run);
        expect(fixture.writes, 1);
        // Real Android Service destruction, not an injected timeout callback.
        await probe.invokeMethod<void>('loseService');
        final paused = await until(
          backend,
          (s) => s.runs.single.state == RunState.paused,
        );
        expect(
          paused.runs.single.attempts.single.state,
          AttemptState.outcomeUnknown,
        );
        await expectLater(
          backend.execute(ResumeRun(run)),
          throwsA(isA<BackendFailure>()),
        );
        expect(fixture.writes, 1);
        await binding.close();
        await backend.close();
        final hosts = <RunServiceRequest>[];
        final nativeServices = PlatformServices.android();
        final reopened = await FrbBackend.open(
          dir.path,
          PlatformServices(
            resolveCredential: nativeServices.resolveCredential,
            setServiceActive: (request) async {
              hosts.add(request);
              await nativeServices.setServiceActive(request);
            },
          ),
        );
        final actions = _ActionBoundary(reopened);
        final available = Completer<Backend>();
        final rebound = EngineLifecycle(() => available.future)..initialize();
        try {
          final recovered = await reopened.snapshot();
          expect(
            recovered.runs.single.attempts.single.state,
            AttemptState.outcomeUnknown,
          );
          await expectLater(
            reopened.execute(ResumeRun(run)),
            throwsA(isA<BackendFailure>()),
          );
          expect(fixture.writes, 1);
          fixture.holdInference = true;
          final nextConversation = (await reopened.execute(
            const CreateConversation(p),
          )).id!;
          final nextProfile = (await reopened.snapshot()).conversations
              .firstWhere((c) => c.id == nextConversation)
              .profile;
          await reopened.execute(
            AcknowledgeContextDisclosure(
              nextConversation,
              nextProfile.inferenceOrigin!,
            ),
          );
          await probe.invokeMethod<void>('finishActivity');
          final requestsBeforeRefusal = fixture.requests;
          final nextRun = (await reopened.execute(
            StartTurn(nextConversation, 'hold inference, no tool write'),
          )).id!;
          await until(
            reopened,
            (s) => s.runs.any(
              (r) => r.id == nextRun && r.state == RunState.paused,
            ),
          );
          expect(fixture.requests, requestsBeforeRefusal);
          final retired = hosts.lastWhere((r) => r.active).generation;
          await probe.invokeMethod<void>('restoreActivity');
          await reopened.execute(ResumeRun(nextRun));
          await fixture.inferenceHeld.future.timeout(
            const Duration(seconds: 20),
          );
          final current = hosts.lastWhere((r) => r.active).generation;
          expect(current, isNot(retired));
          expect(
            (await runtime.invokeMapMethod<String, dynamic>('state'))?['lease'],
            current,
          );
          final retiredDelivered = actions.completed.stream.first;
          available.complete(actions);
          final rejected = await retiredDelivered.timeout(
            const Duration(seconds: 20),
          );
          expect(
            rejected,
            isA<PlatformPause>().having(
              (c) => c.generation,
              'retired native refusal generation',
              retired,
            ),
          );
          expect(actions.conflicts, 1);
          await probe.invokeMethod<void>('dispatchSavedStop');
          expect(
            (await runtime.invokeMapMethod<String, dynamic>('state'))?['runId'],
            nextRun,
          );
          expect(
            (await reopened.snapshot()).runs
                .firstWhere((r) => r.id == nextRun)
                .state,
            RunState.running,
          );
          // Hold an actual native host-loss action after its snapshot but before Core dispatch.
          final entered = Completer<AppCommand>();
          final release = Completer<void>();
          actions.beforeAction = (command) async {
            entered.complete(command);
            await release.future;
          };
          final heldCompleted = actions.completed.stream.first;
          await probe.invokeMethod<void>('loseService');
          final held = await entered.future.timeout(
            const Duration(seconds: 20),
          );
          expect(
            held,
            isA<PlatformPause>().having(
              (c) => c.generation,
              'TOCTOU generation',
              current,
            ),
          );
          await reopened.execute(PlatformPause(nextRun, current));
          await until(
            reopened,
            (s) => s.runs.any(
              (r) => r.id == nextRun && r.state == RunState.paused,
            ),
          );
          fixture.inferenceHeld = Completer<void>();
          await reopened.execute(ResumeRun(nextRun));
          await fixture.inferenceHeld.future.timeout(
            const Duration(seconds: 20),
          );
          final latest = hosts.lastWhere((r) => r.active).generation;
          expect(latest, isNot(current));
          actions.beforeAction = null;
          release.complete();
          await heldCompleted.timeout(const Duration(seconds: 20));
          expect(actions.conflicts, 2);
          expect(
            (await reopened.snapshot()).runs
                .firstWhere((r) => r.id == nextRun)
                .state,
            RunState.running,
          );
          try {
            await probe.invokeMethod<void>('dispatchNotificationStop');
            await until(
              reopened,
              (s) => s.runs.any(
                (r) => r.id == nextRun && r.state == RunState.cancelled,
              ),
            );
            expect(fixture.writes, 1);
          } finally {
            await rebound.close();
          }
        } finally {
          await rebound.close();
          await actions.completed.close();
          await reopened.close();
        }
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        await binding.close();
        await backend.close();
        await fixture.close();
        await dir.delete(recursive: true);
      }
    },
  );
}

// Test scheduling boundary only: every action/snapshot still reaches the same real AgentHandle.
class _ActionBoundary implements Backend {
  _ActionBoundary(this.delegate);
  final Backend delegate;
  Future<void> Function(AppCommand)? beforeAction;
  final completed = StreamController<AppCommand>.broadcast(sync: true);
  int conflicts = 0;
  @override
  Future<CommandReply> execute(AppCommand command) async {
    if (command is! PlatformPause && command is! PlatformStop) {
      return delegate.execute(command);
    }
    try {
      await beforeAction?.call(command);
      return await delegate.execute(command);
    } on BackendFailure catch (error) {
      if (error.code == FailureCode.conflict) conflicts++;
      rethrow;
    } finally {
      completed.add(command);
    }
  }

  @override
  Future<AppSnapshot> snapshot() => delegate.snapshot();
  @override
  Stream<AppEvent> events(int afterSequence) => delegate.events(afterSequence);
  @override
  Future<void> close() => delegate.close();
}

class _LocalWriteFixture {
  _LocalWriteFixture(this.server);
  final HttpServer server;
  int writes = 0;
  int requests = 0;
  bool holdInference = false;
  bool changedSchema = false;
  var inferenceHeld = Completer<void>();
  final written = Completer<void>();
  final held = <HttpResponse>[];
  String get url => 'http://127.0.0.1:${server.port}';
  static Future<_LocalWriteFixture> open() async {
    final fixture = _LocalWriteFixture(
      await HttpServer.bind(InternetAddress.loopbackIPv4, 0),
    );
    fixture.server.listen(fixture.handle);
    return fixture;
  }

  Future<void> handle(HttpRequest request) async {
    try {
      requests++;
      final body =
          jsonDecode(await utf8.decoder.bind(request).join())
              as Map<String, dynamic>;
      if (request.uri.path.endsWith('/chat/completions')) {
        final native = await runtime.invokeMapMethod<String, dynamic>('state');
        if (native?['status'] != 'running') {
          throw StateError(
            'Inference preceded native foreground acknowledgment',
          );
        }
        if (holdInference) {
          held.add(request.response);
          if (!inferenceHeld.isCompleted) inferenceHeld.complete();
          return;
        }
        final functions = (body['tools'] as List).cast<Map>();
        final name =
            functions.singleWhere(
                  (t) => t['function']['description'] == 'write',
                )['function']['name']
                as String;
        request.response.headers.contentType = ContentType(
          'text',
          'event-stream',
        );
        request.response.write(
          'data: ${jsonEncode({
            'choices': [
              {
                'delta': {
                  'role': 'assistant',
                  'tool_calls': [
                    {
                      'index': 0,
                      'id': 'write-once',
                      'type': 'function',
                      'function': {'name': name, 'arguments': '{}'},
                    },
                  ],
                },
                'finish_reason': null,
              },
            ],
          })}\n\n',
        );
        request.response.write(
          'data: ${jsonEncode({
            'choices': [
              {'delta': {}, 'finish_reason': 'tool_calls'},
            ],
          })}\n\ndata: [DONE]\n\n',
        );
      } else {
        final method = body['method'];
        if (method == 'tools/call') {
          writes++;
          held.add(request.response);
          if (!written.isCompleted) written.complete();
          return;
        }
        if (method == 'notifications/initialized') {
          request.response.statusCode = 202;
        } else {
          request.response.headers.contentType = ContentType.json;
          final result = method == 'initialize'
              ? {
                  'protocolVersion': body['params']['protocolVersion'],
                  'capabilities': {'tools': {}},
                  'serverInfo': {'name': 'local', 'version': '1'},
                }
              : {
                  'tools': [
                    {
                      'name': 'write',
                      'description': 'test-owned write counter',
                      'inputSchema': changedSchema
                          ? {
                              'type': 'object',
                              'properties': {
                                'requiredValue': {'type': 'string'},
                              },
                              'required': ['requiredValue'],
                            }
                          : {'type': 'object', 'properties': {}},
                    },
                  ],
                };
          request.response.write(
            jsonEncode({'jsonrpc': '2.0', 'id': body['id'], 'result': result}),
          );
        }
      }
      await request.response.close();
    } catch (_) {
      request.response.statusCode = 500;
      await request.response.close();
    }
  }

  Future<void> close() async {
    for (final response in held) {
      unawaited(response.close().catchError((Object _) {}));
    }
    await server.close(force: true);
  }
}
