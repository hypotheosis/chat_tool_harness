import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/platform_services.dart';
import 'package:chat_tool_harness/features/settings/credential_editor.dart';
import '../support/ui_fixture.dart';

void main() {
  TestDefaultBinaryMessenger messenger() =>
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  final described = <String>[];
  void channel(Object? Function(MethodCall call) answer) {
    messenger().setMockMethodCallHandler(PlatformServices.credentials, (
      call,
    ) async {
      if (call.method == 'describe') {
        described.add(call.arguments['reference'] as String);
      }
      return answer(call);
    });
  }

  setUp(described.clear);
  tearDown(
    () => messenger().setMockMethodCallHandler(
      PlatformServices.credentials,
      null,
    ),
  );

  Future<TextEditingController> pump(
    WidgetTester t,
    String reference, {
    TextEditingController? header,
  }) async {
    final controller = TextEditingController(text: reference);
    addTearDown(controller.dispose);
    await t.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: CredentialEditor(
            backend: UiBackend(),
            reference: controller,
            headerName: header,
          ),
        ),
      ),
    );
    await t.pumpAndSettle();
    return controller;
  }

  testWidgets('shows the effective Authorization header once', (t) async {
    channel((_) => {'scheme': null, 'length': 40, 'sendable': true});
    await pump(t, 'ref-1');
    expect(
      find.text('Authorization: Bearer ••• (40 chars; Bearer added)'),
      findsOneWidget,
    );
    expect(find.textContaining('Authorization: Authorization'), findsNothing);
    expect(described, ['ref-1']);
    await t.pumpWidget(const SizedBox());
  });

  testWidgets('says when no credential is stored', (t) async {
    channel((_) => null);
    await pump(t, 'ref-1');
    expect(find.text('No credential stored'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
  });

  testWidgets('says when the stored credential cannot be read', (t) async {
    channel((call) => throw PlatformException(code: 'unreadable'));
    await pump(t, 'ref-1');
    expect(
      find.text('Stored credential cannot be read; re-enter it.'),
      findsOneWidget,
    );
    await t.pumpWidget(const SizedBox());
  });

  testWidgets('shows nothing without a credential channel', (t) async {
    await pump(t, 'ref-1');
    expect(find.textContaining('Authorization:'), findsNothing);
    expect(find.text('No credential stored'), findsNothing);
    expect(find.textContaining('cannot be read'), findsNothing);
    await t.pumpWidget(const SizedBox());
  });

  testWidgets('empty reference is not described', (t) async {
    channel((_) => null);
    await pump(t, '');
    expect(described, isEmpty);
    expect(find.text('No credential stored'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
  });

  testWidgets('summary follows the reference and refreshes after store', (
    t,
  ) async {
    var stored = false;
    channel(
      (call) => switch (call.method) {
        'put' => () {
          stored = true;
          return null;
        }(),
        _ =>
          stored ? {'scheme': 'Basic', 'length': 10, 'sendable': true} : null,
      },
    );
    final reference = await pump(t, 'ref-1');
    expect(find.text('No credential stored'), findsOneWidget);
    await t.enterText(find.byType(TextField), 'Basic abcd');
    await t.tap(find.text('Store / replace credential'));
    await t.pumpAndSettle();
    expect(find.text('Authorization: Basic ••• (4 chars)'), findsOneWidget);
    reference.text = 'ref-2';
    await t.pump(const Duration(seconds: 1));
    expect(described.last, 'ref-2');
    await t.pumpWidget(const SizedBox());
  });

  testWidgets('typing a reference describes it once, after a pause', (t) async {
    channel((_) => null);
    final reference = await pump(t, '');
    for (final text in ['r', 're', 'ref', 'ref-', 'ref-9']) {
      reference.text = text;
      await t.pump(const Duration(milliseconds: 100));
    }
    expect(described, isEmpty);
    await t.pump(const Duration(seconds: 1));
    expect(described, ['ref-9']);
    reference.selection = const TextSelection.collapsed(offset: 2);
    await t.pump(const Duration(seconds: 1));
    expect(described, ['ref-9']);
    await t.pumpWidget(const SizedBox());
  });

  testWidgets('other header names are previewed verbatim', (t) async {
    channel((_) => {'scheme': null, 'length': 12, 'sendable': false});
    final header = TextEditingController(text: 'X-Api-Key');
    addTearDown(header.dispose);
    await pump(t, 'ref-1', header: header);
    expect(find.textContaining('X-Api-Key: ••• (12 chars)'), findsOneWidget);
    expect(find.textContaining('cannot be sent'), findsOneWidget);
    expect(find.textContaining('Bearer'), findsNothing);
    header.text = 'authorization';
    await t.pumpAndSettle();
    expect(find.textContaining('Authorization: Bearer'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
  });
}
