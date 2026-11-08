import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/features/settings/endpoint_screen.dart';
import 'package:chat_tool_harness/features/settings/server_screen.dart';
import '../support/ui_fixture.dart';

/// A 360 x 640 phone with a 48 px navigation bar at the bottom.
const height = 640.0;
const navigationBar = 48.0;
const visibleBottom = height - navigationBar;

void phone(WidgetTester t) {
  t.view.physicalSize = const Size(1080, height * 3);
  t.view.devicePixelRatio = 3;
  t.view.padding = const FakeViewPadding(bottom: navigationBar * 3);
  t.view.viewPadding = const FakeViewPadding(bottom: navigationBar * 3);
  addTearDown(t.view.reset);
}

Future<void> scrollToEnd(WidgetTester t) async {
  final position = t
      .state<ScrollableState>(find.byType(Scrollable).first)
      .position;
  while (position.pixels < position.maxScrollExtent) {
    position.jumpTo(position.maxScrollExtent);
    await t.pumpAndSettle();
  }
}

Future<void> openServer(WidgetTester t, UiBackend b) async {
  phone(t);
  await t.pumpWidget(
    MaterialApp(
      home: ServerScreen(backend: b, server: b.current.servers.single),
    ),
  );
  await t.pumpAndSettle();
  await scrollToEnd(t);
  await t.tap(find.byType(DropdownButtonFormField<String>));
  await t.pumpAndSettle();
  await t.tap(find.text('Profile • p').last);
  await t.pumpAndSettle();
  await scrollToEnd(t);
}

Future<void> openEndpoint(WidgetTester t, UiBackend b) async {
  phone(t);
  await t.pumpWidget(
    MaterialApp(
      home: EndpointScreen(backend: b, endpoint: endpoint),
    ),
  );
  await t.pumpAndSettle();
  await scrollToEnd(t);
}

void expectOnScreen(WidgetTester t, Finder finder) {
  final rect = t.getRect(finder);
  expect(rect.top, greaterThanOrEqualTo(0));
  expect(rect.bottom, lessThanOrEqualTo(visibleBottom));
}

const failure = BackendFailure(
  FailureCode.authentication,
  detail: FailureDetail(
    targetKind: 'mcpServer',
    targetName: 'tools',
    step: 'initialize',
    status: 401,
    reason: 'unknown API key',
  ),
);

void main() {
  testWidgets('server failure scrolls into view above the inset', (t) async {
    final b = UiBackend();
    await openServer(t, b);
    b.failure = failure;
    await t.tap(find.text('Test connection and discover tools'));
    await t.pumpAndSettle();
    expectOnScreen(t, find.text('Failed'));
  });

  testWidgets('server success scrolls into view above the inset', (t) async {
    final b = UiBackend()
      ..reply = const CommandReply(
        discoveredTools: ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'],
      );
    await openServer(t, b);
    await t.tap(find.text('Test connection and discover tools'));
    await t.pumpAndSettle();
    expectOnScreen(t, find.textContaining('8 tools available'));
  });

  testWidgets('endpoint failure scrolls into view above the inset', (t) async {
    final b = UiBackend();
    await openEndpoint(t, b);
    b.failure = failure;
    await t.tap(find.text('Discover models'));
    await t.pumpAndSettle();
    expectOnScreen(t, find.text('Failed'));
  });

  testWidgets('endpoint success scrolls into view above the inset', (t) async {
    final b = UiBackend()
      ..reply = const CommandReply(models: ['a', 'b', 'c', 'd', 'e', 'f']);
    await openEndpoint(t, b);
    await t.tap(find.text('Discover models'));
    await t.pumpAndSettle();
    expectOnScreen(t, find.textContaining('6 models available'));
  });

  testWidgets('progress is visible while an action is pending', (t) async {
    final pending = Completer<CommandReply>();
    final b = PendingBackend(pending);
    await openEndpoint(t, b);
    await t.tap(find.text('Discover models'));
    await t.pump();
    expectOnScreen(t, find.byType(LinearProgressIndicator));
    pending.complete(const CommandReply());
    await t.pumpAndSettle();
    expect(find.byType(LinearProgressIndicator), findsNothing);
  });
}

class PendingBackend extends UiBackend {
  PendingBackend(this.pending);
  final Completer<CommandReply> pending;
  @override
  Future<CommandReply> execute(AppCommand c) =>
      c is DiscoverModels ? pending.future : super.execute(c);
}
