import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:chat_tool_harness/backend/frb_backend.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/backend/platform_services.dart';
import 'package:chat_tool_harness/features/chat/chat_screen.dart';
import 'package:chat_tool_harness/features/files/file_controller.dart';
import 'package:chat_tool_harness/features/files/file_panel.dart';
import 'package:chat_tool_harness/features/history/history_screen.dart';
import 'package:chat_tool_harness/src/rust/frb_generated.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

const _secret = 'task14-synthetic-key-not-personal';
const _markdown = '# Controlled summary\n\n- Read complete\n- Write approved';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() => RustLib.init());
  testWidgets('android_mcp_markdown_workflow', (tester) async {
    debugPrint('TASK14_OWNED_PID=$pid');
    final dir = await Directory.systemTemp.createTemp('task14-core-');
    final fixture = await _Fixture.open();
    final backend = await FrbBackend.open(
      dir.path,
      PlatformServices.android(resolveCredential: (_) async => _secret),
    );
    try {
      Future<String> id() async => (await backend.execute(const NewId())).id!;
      final endpoint = await id(), server = await id(), profile = await id();
      await backend.execute(
        SaveEndpoint(
          EndpointConfig(
            id: endpoint,
            name: 'controlled',
            baseUrl: fixture.url,
            model: 'fixture',
            contextTokens: 32000,
            maxOutputTokens: 1024,
            temperature: 0,
            allowHttp: true,
            credential: await id(),
          ),
        ),
      );
      await backend.execute(
        SaveServer(
          ServerConfig(
            id: server,
            name: 'controlled MCP',
            url: '${fixture.url}/mcp',
            allowHttp: true,
          ),
        ),
      );
      await backend.execute(
        SaveProfile(
          Profile(
            id: profile,
            name: 'Task14',
            endpointId: endpoint,
            serverIds: [server],
            instructions: '',
            limits: const RunLimits(),
          ),
        ),
      );
      final owner = (await backend.execute(CreateConversation(profile))).id!;
      Future<void> screen(Widget widget) async {
        await tester.pumpWidget(MaterialApp(key: UniqueKey(), home: widget));
        await tester.pumpAndSettle();
      }

      Future<void> chat() async {
        await screen(ChatScreen(backend: backend, conversationId: owner));
        for (
          var i = 0;
          i < 200 &&
              find.text('Current profile model: fixture').evaluate().isEmpty;
          i++
        ) {
          await tester.pump(const Duration(milliseconds: 20));
        }
        expect(find.text('Current profile model: fixture'), findsOneWidget);
      }

      Future<void> tap(Finder finder) async {
        if (finder.evaluate().isEmpty) {
          await tester.scrollUntilVisible(
            finder,
            300,
            maxScrolls: 100,
            scrollable: find.byType(Scrollable).first,
          );
        }
        await tester.ensureVisible(finder);
        await tester.pumpAndSettle();
        await tester.tap(finder);
        await tester.pumpAndSettle();
      }

      Future<Run> waitRun(bool Function(Run) predicate) async {
        for (var i = 0; i < 600; i++) {
          final snapshot = await backend.snapshot();
          final runs = snapshot.runs
              .where((r) => r.conversationId == owner)
              .toList();
          if (runs.isNotEmpty && predicate(runs.last)) return runs.last;
          await tester.pump(const Duration(milliseconds: 50));
        }
        final s = await backend.snapshot();
        throw StateError(
          'Workflow state: ${s.runs.map((r) => '${r.state}/${r.failure}').join(', ')}; fixture errors ${fixture.errors}',
        );
      }

      String? lastRun;
      Future<Run> send(String prompt, bool Function(Run) predicate) async {
        await chat();
        final field = find.byKey(const Key('message'));
        if (field.evaluate().isEmpty) {
          await tester.scrollUntilVisible(
            field,
            500,
            maxScrolls: 100,
            scrollable: find.byType(Scrollable).first,
          );
        }
        await tester.ensureVisible(field);
        await tester.enterText(field, prompt);
        await SystemChannels.textInput.invokeMethod<void>('TextInput.hide');
        await tester.pumpAndSettle();
        await tap(find.text('Send'));
        return waitRun((r) => r.id != lastRun && predicate(r));
      }

      Future<void> policy(String name, PermissionRule rule) async {
        final discovery = await backend.execute(
          DiscoverServer(profile, server),
        );
        final observed = discovery.discoveredRules.singleWhere(
          (r) => r.key.name == name,
        );
        await backend.execute(
          SetRule(
            observed.key,
            observed.schemaDigest,
            observed.destinationDigest,
            rule,
          ),
        );
      }

      await chat();
      await tap(find.byKey(const Key('ack-new-work')));
      var run = await send('read', (r) => r.state == RunState.waitingApproval);
      lastRun = run.id;
      expect(
        fixture.calls,
        isEmpty,
        reason: 'New read tools must default to Ask',
      );
      expect(
        run.attempts.single.binding!.argumentsJson,
        '{"value":"read-exact"}',
      );
      Future<void> approve(Run pending) async {
        await chat();
        final tile = find.textContaining(
          'Tool ${pending.attempts.single.call.key.name} •',
        );
        await tap(tile);
        expect(find.text('Requested arguments (immutable)'), findsOneWidget);
        await tap(find.text('Approve exact arguments'));
        await tap(find.byKey(Key('resume-${pending.id}')));
        await waitRun(
          (r) => r.id == pending.id && r.state == RunState.completed,
        );
      }

      await approve(run);
      expect(fixture.calls.single, {
        'name': 'read',
        'arguments': {'value': 'read-exact'},
      });
      run = await send('ask', (r) => r.state == RunState.waitingApproval);
      lastRun = run.id;
      expect(fixture.writes, 0);
      final binding = run.attempts.single.binding!;
      final changed = ApprovalBinding(
        attemptId: binding.attemptId,
        key: binding.key,
        providerCallId: binding.providerCallId,
        definitionDigest: binding.definitionDigest,
        argumentsDigest: binding.argumentsDigest,
        argumentsJson: '{"value":"changed"}',
        schemaJson: binding.schemaJson,
        destinationDigest: binding.destinationDigest,
      );
      await expectLater(
        backend.execute(Approve(changed)),
        throwsA(isA<BackendFailure>()),
      );
      expect(fixture.writes, 0, reason: 'Forged approval cannot dispatch');
      await approve(run);
      expect(fixture.writes, 1);
      expect(fixture.calls.last['arguments'], {'value': 'ask-exact'});
      await policy('write', PermissionRule.allow);
      run = await send('auto', (r) => r.state == RunState.completed);
      lastRun = run.id;
      expect(
        fixture.writes,
        2,
        reason: 'Explicit Allow automatically dispatches exact write',
      );
      expect(fixture.calls.last['arguments'], {'value': 'auto-exact'});
      await policy('write', PermissionRule.block);
      run = await send('block', (r) => r.state != RunState.running);
      lastRun = run.id;
      expect(run.attempts.single.state, AttemptState.denied);
      expect(fixture.writes, 2);
      run = await send('markdown', (r) => r.state == RunState.waitingApproval);
      lastRun = run.id;
      await approve(run);
      final artifacts = (await backend.snapshot()).artifacts;
      expect(artifacts.single.name, 'summary.md');
      await screen(
        FilePanel(controller: FileController(backend, _NoDelivery(), owner)),
      );
      await tap(find.text('summary.md'));
      expect(find.text(_markdown), findsOneWidget);
      expect(
        find.text(
          'Literal text preview — no HTML, image loading or link navigation.',
        ),
        findsOneWidget,
      );
      run = await send(
        'stream',
        (r) => r.partialText.contains('controlled partial'),
      );
      lastRun = run.id;
      await chat();
      await tester.scrollUntilVisible(
        find.text('controlled partial'),
        500,
        maxScrolls: 100,
        scrollable: find.byType(Scrollable).first,
      );
      expect(find.text('controlled partial'), findsOneWidget);
      final notifications = await const MethodChannel(
        'chat_tool_harness/agent_probe',
      ).invokeListMethod<dynamic>('notificationState');
      expect(jsonEncode(notifications), isNot(contains(_secret)));
      await tap(find.byKey(Key('stop-${run.id}')));
      final stopped = await waitRun(
        (r) => r.id == run.id && r.state != RunState.running,
      );
      expect(stopped.state, RunState.cancelled);
      expect(stopped.partialText, 'controlled partial');
      expect(fixture.calls.length, 3);
      await policy('write', PermissionRule.allow);
      run = await send(
        'unknown',
        (r) => r.attempts.any((a) => a.state == AttemptState.dispatching),
      );
      lastRun = run.id;
      await fixture.unknownWritten.future.timeout(const Duration(seconds: 10));
      await chat();
      await tap(find.byKey(Key('stop-${run.id}')));
      final unknown = await waitRun(
        (r) => r.id == run.id && r.state != RunState.running,
      );
      expect(unknown.attempts.single.state, AttemptState.outcomeUnknown);
      final requests = fixture.requests;
      await expectLater(
        backend.execute(ResumeRun(run.id)),
        throwsA(isA<BackendFailure>()),
      );
      expect(fixture.requests, requests);
      expect(fixture.writes, 3);
      // Offline means no listening fixture remains; history/export must still work.
      await fixture.close();
      await screen(HistoryScreen(backend: backend));
      await tap(find.byTooltip('Rename'));
      await tester.enterText(
        find.byKey(const Key('history-title')),
        'Core offline checkpoint',
      );
      await tap(find.widgetWithText(FilledButton, 'Rename'));
      await tester.enterText(
        find.byKey(const Key('history-search')),
        'Core offline',
      );
      await tap(find.text('Search'));
      expect(find.text('Core offline checkpoint'), findsOneWidget);
      await tap(find.byTooltip('Export transcript'));
      await tap(find.text('Preview'));
      final bundle = (await backend.execute(
        ExportTranscript(owner),
      )).exportBundle!;
      final exported = (await backend.execute(
        ReadArtifact(owner, bundle.transcript.id),
      )).artifact!.text;
      expect(exported, contains('not a restorable backup'));
      expect(exported, isNot(contains(_secret)));
      final snapshot = await backend.snapshot();
      expect(
        snapshot.conversations.single.messages.map((m) => m.content).join(),
        isNot(contains(_secret)),
      );
      expect(fixture.authenticated, greaterThan(0));
      expect(fixture.errors, isEmpty);
      await tap(find.text('Close'));
      await tap(find.text('Core offline checkpoint'));
      expect(find.text('Chat'), findsOneWidget);
      // A new offline turn fails honestly without deleting the existing transcript.
      final before = snapshot.conversations.single.messages.length;
      final offline = await backend.execute(
        StartTurn(owner, 'offline unavailable'),
      );
      final unavailable = await waitRun(
        (r) => r.id == offline.id && r.state != RunState.running,
      );
      expect(unavailable.state, anyOf(RunState.failed, RunState.paused));
      expect(unavailable.failure, isNotNull);
      expect(
        (await backend.snapshot()).conversations.single.messages.length,
        greaterThanOrEqualTo(before),
      );
      expect(
        (await backend.snapshot()).artifacts.any((a) => a.name == 'summary.md'),
        isTrue,
      );
      expect(fixture.requests, requests);
    } finally {
      await tester.pumpWidget(const SizedBox());
      await tester.pumpAndSettle();
      await backend.close();
      await fixture.close();
      await dir.delete(recursive: true);
    }
  });
}

class _NoDelivery implements AndroidDocumentAccess {
  @override
  Future<DocumentResult> saveArtifact(ArtifactContent artifact) =>
      throw StateError('External save is outside automation');
  @override
  Future<DocumentResult> shareArtifact(ArtifactContent artifact) =>
      throw StateError('External share is outside automation');
}

class _Fixture {
  _Fixture(this.server);
  final HttpServer server;
  final calls = <Map<String, dynamic>>[];
  final errors = <String>[];
  final held = <HttpResponse>[];
  final unknownWritten = Completer<void>();
  int requests = 0, writes = 0, authenticated = 0;
  bool closed = false;
  String get url => 'http://127.0.0.1:${server.port}';
  static Future<_Fixture> open() async {
    final fixture = _Fixture(
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
        if (request.headers.value('authorization') != 'Bearer $_secret') {
          throw StateError('Missing synthetic authentication');
        }
        authenticated++;
        final messages = (body['messages'] as List).cast<Map>();
        final prompt =
            messages.lastWhere((m) => m['role'] == 'user')['content'] as String;
        request.response.headers.contentType = ContentType(
          'text',
          'event-stream',
        );
        Map<String, dynamic> delta;
        String finish;
        if (prompt == 'stream') {
          request.response.bufferOutput = false;
          delta = {'role': 'assistant', 'content': 'controlled partial'};
          request.response.write(
            'data: ${jsonEncode({
              'choices': [
                {'delta': delta, 'finish_reason': null},
              ],
            })}\n\n',
          );
          await request.response.flush();
          held.add(request.response);
          return;
        } else if (messages.last['role'] == 'tool') {
          delta = {'content': 'Controlled operation complete'};
          finish = 'stop';
        } else {
          final tool = prompt == 'read'
              ? 'read'
              : prompt == 'markdown'
              ? 'create_artifact'
              : 'write';
          final functions = (body['tools'] as List).cast<Map>();
          final name = functions.singleWhere(
            (t) => t['function']['description'] == tool,
          )['function']['name'];
          final args = tool == 'create_artifact'
              ? {'name': 'summary.md', 'content': _markdown}
              : {'value': '$prompt-exact'};
          delta = {
            'tool_calls': [
              {
                'index': 0,
                'id': 'controlled-$prompt',
                'type': 'function',
                'function': {'name': name, 'arguments': jsonEncode(args)},
              },
            ],
          };
          finish = 'tool_calls';
        }
        request.response.write(
          'data: ${jsonEncode({
            'choices': [
              {'delta': delta, 'finish_reason': finish},
            ],
          })}\n\ndata: [DONE]\n\n',
        );
      } else {
        final method = body['method'];
        if (method == 'notifications/initialized') {
          request.response.statusCode = 202;
        } else {
          Object result;
          if (method == 'initialize') {
            result = {
              'protocolVersion': body['params']['protocolVersion'],
              'capabilities': {'tools': {}},
              'serverInfo': {'name': 'controlled', 'version': '1'},
            };
          } else if (method == 'tools/list') {
            result = {
              'tools': [
                for (final name in ['read', 'write'])
                  {
                    'name': name,
                    'description': 'controlled $name',
                    'inputSchema': {
                      'type': 'object',
                      'properties': {
                        'value': {'type': 'string'},
                      },
                      'required': ['value'],
                      'additionalProperties': false,
                    },
                  },
              ],
            };
          } else if (method == 'tools/call') {
            final params = Map<String, dynamic>.from(body['params'] as Map);
            calls.add(params);
            if (params['name'] == 'write') writes++;
            if (params['arguments']['value'] == 'unknown-exact') {
              held.add(request.response);
              unknownWritten.complete();
              return;
            }
            result = {
              'content': [
                {'type': 'text', 'text': 'controlled result'},
              ],
              'isError': false,
            };
          } else {
            throw StateError('Unexpected MCP method $method');
          }
          request.response.headers.contentType = ContentType.json;
          request.response.write(
            jsonEncode({'jsonrpc': '2.0', 'id': body['id'], 'result': result}),
          );
        }
      }
      await request.response.close();
    } catch (e) {
      errors.add('$e');
      request.response.statusCode = 500;
      await request.response.close();
    }
  }

  Future<void> close() async {
    if (closed) return;
    closed = true;
    for (final response in held) {
      unawaited(response.close().catchError((Object _) {}));
    }
    await server.close(force: true);
  }
}
