import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/engine_lifecycle.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/backend/platform_services.dart';
import 'package:chat_tool_harness/features/settings/credential_editor.dart';
import 'support/ui_fixture.dart';

void main() {
  testWidgets(
    'secure editor replaces/deletes without plaintext model commands',
    (t) async {
      final backend = UiBackend();
      final reference = TextEditingController(text: 'reference-1');
      final calls = <String>[];
      final values = <String>[];
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(PlatformServices.credentials, (call) async {
            calls.add(call.method);
            if (call.method == 'put') {
              values.add(utf8.decode(call.arguments['value'] as List<int>));
            }
            return null;
          });
      addTearDown(() {
        reference.dispose();
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
            .setMockMethodCallHandler(PlatformServices.credentials, null);
      });
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: CredentialEditor(backend: backend, reference: reference),
          ),
        ),
      );
      final field = find.byType(TextField);
      expect(t.widget<TextField>(field).obscureText, true);
      expect(t.widget<TextField>(field).enableIMEPersonalizedLearning, false);
      await t.enterText(field, 'test-token');
      await t.tap(find.text('Store / replace credential'));
      await t.pumpAndSettle();
      expect(values, ['test-token']);
      expect(t.widget<TextField>(field).controller!.text, isEmpty);
      expect(backend.commands, isEmpty);
      expect(reference.text, 'reference-1');
      await t.tap(find.text('Delete stored credential'));
      await t.pumpAndSettle();
      expect(calls, ['put']);
      await t.tap(find.text('Confirm deletion'));
      await t.pumpAndSettle();
      expect(calls, ['put', 'delete']);
      expect(find.textContaining('history remains available'), findsOneWidget);
      await t.pumpWidget(const SizedBox());
    },
  );
  test(
    'engine binding is exact-run and Open never approves or resumes',
    () async {
      final backend = UiBackend()
        ..current = AppSnapshot(
          conversations: [conversation],
          runs: [run(runState: RunState.running)],
        );
      final binding = EngineLifecycle(() async => backend);
      await binding.handle({
        'action': 'stop',
        'runId': 'old',
        'conversationId': 'c',
      });
      await binding.handle({
        'action': 'stop',
        'runId': 'r',
        'conversationId': 'wrong',
      });
      expect(backend.commands, isEmpty);
      await binding.handle({
        'action': 'stop',
        'runId': 'r',
        'conversationId': 'c',
      });
      expect(
        backend.commands,
        isEmpty,
        reason: 'Stop requires generation authority; Open does not',
      );
      await binding.handle({
        'action': 'open',
        'runId': 'r',
        'conversationId': 'c',
      });
      expect(binding.openConversation.value, 'c');
      expect(backend.commands, isEmpty);
      await binding.handle({
        'action': 'timeout',
        'lease': 'generation-a',
        'runId': 'r',
        'conversationId': 'c',
      });
      expect(backend.commands.single, isA<PlatformPause>());
      backend.commands.clear();
      await binding.handle({
        'action': 'stop',
        'lease': 'generation-a',
        'runId': 'r',
        'conversationId': 'c',
      });
      expect(backend.commands.single, isA<PlatformStop>());
      backend.commands.clear();
      backend.current = AppSnapshot(runs: [run()]);
      await binding.handle({
        'action': 'stop',
        'lease': 'generation-a',
        'runId': 'r',
        'conversationId': 'c',
      });
      expect(backend.commands, isEmpty);
    },
  );
}
