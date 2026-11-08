import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/settings/endpoint_screen.dart';
import '../support/ui_fixture.dart';

void main() {
  testWidgets('configure_manual_model_and_token_reference', (t) async {
    final b = UiBackend();
    await t.pumpWidget(
      MaterialApp(
        home: EndpointScreen(backend: b, endpoint: endpoint),
      ),
    );
    await t.pumpAndSettle();
    await t.enterText(
      find.widgetWithText(TextFormField, 'Model ID'),
      'custom-model',
    );
    await t.enterText(
      find.widgetWithText(
        TextFormField,
        'Credential reference (UUID, not a token)',
      ),
      'uuid-reference',
    );
    await t.scrollUntilVisible(
      find.text('Save endpoint'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.tap(find.text('Save endpoint'));
    await t.pumpAndSettle();
    final saved = b.commands.whereType<SaveEndpoint>().single.endpoint;
    expect(saved.model, 'custom-model');
    expect(saved.credential, 'uuid-reference');
    expect(find.textContaining('raw-secret'), findsNothing);
  });
  testWidgets('connection_test_shows_auth_vs_tool_support', (t) async {
    final b = UiBackend()
      ..failure = const BackendFailure(FailureCode.authentication);
    await t.pumpWidget(
      MaterialApp(
        home: EndpointScreen(backend: b, endpoint: endpoint),
      ),
    );
    await t.pumpAndSettle();
    await t.scrollUntilVisible(
      find.text('Probe tool support'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.tap(find.text('Probe tool support'));
    await t.pumpAndSettle();
    expect(find.textContaining('Authentication'), findsOneWidget);
    expect(b.commands.whereType<ProbeEndpoint>(), hasLength(1));
    await t.scrollUntilVisible(
      find.text('Probe tool support'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    b.failure = null;
    b.reply = const CommandReply(toolSupport: ToolSupport.unsupported);
    await t.tap(find.text('Probe tool support'));
    await t.pumpAndSettle();
    expect(find.textContaining('Unsupported'), findsOneWidget);
    expect(b.commands.whereType<SaveProfile>(), isEmpty);
  });
  testWidgets('failed model discovery shows endpoint, status and reason', (
    t,
  ) async {
    final b = UiBackend()
      ..failure = const BackendFailure(
        FailureCode.authentication,
        detail: FailureDetail(
          targetKind: 'inference',
          targetName: 'Endpoint',
          step: 'discoverModels',
          status: 403,
          reason: 'key revoked',
        ),
      );
    await t.pumpWidget(
      MaterialApp(
        home: EndpointScreen(backend: b, endpoint: endpoint),
      ),
    );
    await t.pumpAndSettle();
    await t.scrollUntilVisible(
      find.text('Discover models'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.tap(find.text('Discover models'));
    await t.pumpAndSettle();
    expect(
      find.textContaining('Inference endpoint "Endpoint"'),
      findsOneWidget,
    );
    expect(
      find.textContaining('HTTP 403 during model discovery'),
      findsOneWidget,
    );
    expect(find.textContaining('key revoked'), findsOneWidget);
    expect(find.text('Details'), findsOneWidget);
  });
  testWidgets('successful model discovery shows model count', (t) async {
    final b = UiBackend()..reply = const CommandReply(models: ['a', 'b', 'c']);
    await t.pumpWidget(
      MaterialApp(
        home: EndpointScreen(backend: b, endpoint: endpoint),
      ),
    );
    await t.pumpAndSettle();
    await t.scrollUntilVisible(
      find.text('Discover models'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.tap(find.text('Discover models'));
    await t.pumpAndSettle();
    expect(find.textContaining('3 models available'), findsOneWidget);
  });
  testWidgets('new endpoints default to 32768 context and 2048 output', (
    t,
  ) async {
    await t.pumpWidget(MaterialApp(home: EndpointScreen(backend: UiBackend())));
    await t.pumpAndSettle();
    expect(find.widgetWithText(TextFormField, '32768'), findsOneWidget);
    expect(find.widgetWithText(TextFormField, '2048'), findsOneWidget);
  });
  testWidgets('discovery_loading_and_error_are_safe', (t) async {
    final pending = Completer<CommandReply>();
    final b = DelayedBackend(pending);
    await t.pumpWidget(
      MaterialApp(
        home: EndpointScreen(backend: b, endpoint: endpoint),
      ),
    );
    await t.pumpAndSettle();
    await t.scrollUntilVisible(
      find.text('Discover models'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    await t.tap(find.text('Discover models'));
    await t.pump();
    expect(find.byType(LinearProgressIndicator), findsOneWidget);
    pending.completeError(Exception('raw-secret'));
    await t.pumpAndSettle();
    expect(find.textContaining('raw-secret'), findsNothing);
    expect(find.textContaining('unavailable'), findsWidgets);
  });
}

class DelayedBackend extends UiBackend {
  DelayedBackend(this.pending);
  final Completer<CommandReply> pending;
  @override
  Future<CommandReply> execute(AppCommand c) => pending.future;
}
