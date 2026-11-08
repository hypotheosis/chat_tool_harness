import 'dart:async';
import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/backend/platform_services.dart';
import 'package:chat_tool_harness/features/settings/credential_editor.dart';
import 'package:chat_tool_harness/features/settings/server_screen.dart';
import 'support/ui_fixture.dart';

class OwnedController extends TextEditingController {
  OwnedController({super.text});
  bool retired = false;
  int lateAccesses = 0;
  @override
  String get text {
    if (retired) lateAccesses++;
    return super.text;
  }

  @override
  set text(String value) {
    if (retired) lateAccesses++;
    super.text = value;
  }

  @override
  void dispose() {
    super.dispose();
    retired = true;
  }
}

class PendingIdBackend extends UiBackend {
  final id = Completer<CommandReply>();
  @override
  Future<CommandReply> execute(AppCommand command) =>
      command is NewId ? id.future : super.execute(command);
}

void main() {
  testWidgets(
    'removing first header preserves second token/reference native put pair',
    (t) async {
      t.view.physicalSize = const Size(1000, 2400);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.resetPhysicalSize);
      addTearDown(t.view.resetDevicePixelRatio);
      final puts = <(String, String)>[];
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(PlatformServices.credentials, (call) async {
            // The editor's credential summary is not under test here.
            if (call.method == 'describe') return null;
            puts.add((
              call.arguments['reference'] as String,
              utf8.decode(call.arguments['value'] as List<int>),
            ));
            return null;
          });
      addTearDown(
        () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
            .setMockMethodCallHandler(PlatformServices.credentials, null),
      );
      await t.pumpWidget(
        MaterialApp(
          home: ServerScreen(
            backend: UiBackend(),
            server: const ServerConfig(
              id: 'server',
              name: 'test',
              url: 'https://example.invalid/mcp',
              allowHttp: false,
              secretHeaders: [
                SecretHeader(name: 'X-A', credential: 'ref-a'),
                SecretHeader(name: 'X-B', credential: 'ref-b'),
              ],
            ),
          ),
        ),
      );
      await t.pumpAndSettle();
      final secrets = find.descendant(
        of: find.byType(CredentialEditor),
        matching: find.byType(TextField),
      );
      await t.enterText(secrets.at(0), 'token-a');
      await t.enterText(secrets.at(1), 'token-b');
      await t.tap(find.text('Remove header').first);
      await t.pumpAndSettle();
      expect(t.widget<TextField>(secrets).controller!.text, 'token-b');
      await t.tap(find.text('Store / replace credential'));
      await t.pumpAndSettle();
      expect(puts, [('ref-b', 'token-b')]);
      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'retired NewId cannot regain ownership when the original controller returns',
    (t) async {
      final backend = PendingIdBackend();
      final a = OwnedController();
      final b = OwnedController(text: 'ref-b');
      final puts = <String>[];
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(PlatformServices.credentials, (call) async {
            // The editor's credential summary is not under test here.
            if (call.method == 'describe') return null;
            puts.add(call.arguments['reference'] as String);
            return null;
          });
      addTearDown(() {
        a.dispose();
        b.dispose();
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
            .setMockMethodCallHandler(PlatformServices.credentials, null);
      });
      Widget editor(TextEditingController ref) => MaterialApp(
        home: Scaffold(
          body: CredentialEditor(backend: backend, reference: ref),
        ),
      );
      await t.pumpWidget(editor(a));
      await t.enterText(find.byType(TextField), 'retired-token-a');
      await t.tap(find.text('Store / replace credential'));
      await t.pumpAndSettle();
      await t.pumpWidget(editor(b));
      await t.pumpWidget(editor(a));
      await t.enterText(find.byType(TextField), 'new-unsaved-token-a');
      backend.id.complete(const CommandReply(id: 'retired-generated-id'));
      await t.pumpAndSettle();
      expect(puts, isEmpty);
      expect(a.text, isEmpty);
      expect(
        t.widget<TextField>(find.byType(TextField)).controller!.text,
        'new-unsaved-token-a',
      );
      await t.pumpWidget(const SizedBox());
    },
  );

  for (final operation in ['put', 'delete', 'newId']) {
    for (final replace in [false, true]) {
      for (final fail in [false, true]) {
        testWidgets(
          '$operation pending ${replace ? 'replacement' : 'unmount'} ${fail ? 'failure' : 'success'} never touches retired controllers',
          (t) async {
            final backend = PendingIdBackend();
            final a = OwnedController(
              text: operation == 'newId' ? '' : 'ref-a',
            );
            final b = TextEditingController(text: 'ref-b');
            final pending = Completer<void>();
            final calls = <(String, String)>[];
            TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
                .setMockMethodCallHandler(PlatformServices.credentials, (
                  call,
                ) async {
                  // The editor's credential summary is not under test here.
                  if (call.method == 'describe') return null;
                  calls.add((
                    call.method,
                    call.arguments['reference'] as String,
                  ));
                  await pending.future;
                  return null;
                });
            addTearDown(() {
              b.dispose();
              TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
                  .setMockMethodCallHandler(PlatformServices.credentials, null);
            });
            Widget editor(TextEditingController ref) => MaterialApp(
              home: Scaffold(
                body: CredentialEditor(backend: backend, reference: ref),
              ),
            );
            await t.pumpWidget(editor(a));
            await t.enterText(find.byType(TextField), 'token-a');
            await t.tap(
              find.text(
                operation == 'delete'
                    ? 'Delete stored credential'
                    : 'Store / replace credential',
              ),
            );
            await t.pumpAndSettle();
            if (operation == 'delete') {
              await t.tap(find.text('Confirm deletion'));
              await t.pumpAndSettle();
            }
            await t.pumpWidget(replace ? editor(b) : const SizedBox());
            a.dispose();
            if (replace) {
              await t.enterText(find.byType(TextField), 'token-b');
            }
            if (operation == 'newId') {
              if (fail) {
                backend.id.completeError(StateError('test ID failure'));
              } else {
                backend.id.complete(const CommandReply(id: 'generated-for-a'));
              }
            } else if (fail) {
              pending.completeError(StateError('test native failure'));
            } else {
              pending.complete();
            }
            await t.pumpAndSettle();
            expect(t.takeException(), isNull);
            expect(a.lateAccesses, 0);
            expect(b.text, 'ref-b');
            if (operation == 'newId') {
              expect(calls, isEmpty);
            } else {
              expect(calls, [(operation, 'ref-a')]);
            }
            if (replace) {
              expect(
                t.widget<TextField>(find.byType(TextField)).controller!.text,
                'token-b',
              );
            }
            await t.pumpWidget(const SizedBox());
          },
        );
      }
    }
  }
}
