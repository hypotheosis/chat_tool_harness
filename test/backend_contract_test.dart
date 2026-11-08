import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/backend.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'support/fake_backend.dart';

void main() {
  test('older_equal_sequence_refresh_cannot_undo_post_save_snapshot', () async {
    final fake = FakeBackend();
    final session = BackendSession(fake);
    final old = Completer<AppSnapshot>();
    final updated = Completer<AppSnapshot>();
    fake.snapshotResponses.addAll([old.future, updated.future]);
    const endpoint = EndpointConfig(
      id: 'e',
      name: 'saved',
      baseUrl: 'https://example.invalid',
      model: 'test',
      contextTokens: 1,
      maxOutputTokens: 1,
      temperature: 0,
      allowHttp: false,
    );
    final observed = <AppSnapshot>[];
    final listener = session.changes.listen(observed.add);
    final earlier = session.refresh();
    final save = session.execute(const SaveEndpoint(endpoint));
    await Future<void>.delayed(Duration.zero);
    updated.complete(const AppSnapshot(sequence: 7, endpoints: [endpoint]));
    await save;
    old.complete(const AppSnapshot(sequence: 7));
    await earlier;
    await Future<void>.delayed(Duration.zero);
    expect(session.current!.endpoints.single.name, 'saved');
    expect(observed, hasLength(1));
    expect(observed.single.endpoints.single.name, 'saved');
    await listener.cancel();
    await session.dispose();
    await fake.close();
  });
  test('snapshot_gap_refresh', () async {
    final fake = FakeBackend();
    final session = BackendSession(fake);
    await session.connect();
    fake.emit(const AppEvent(sequence: 1, conversationId: 'c'));
    await Future<void>.delayed(Duration.zero);
    expect(session.cursor, 1);
    fake.current = const AppSnapshot(sequence: 4);
    fake.emit(const AppEvent(sequence: 4, conversationId: 'c'));
    await Future<void>.delayed(Duration.zero);
    expect(fake.snapshotCalls, 3);
    expect(session.cursor, 4);
    fake.emit(const AppEvent(sequence: 3, conversationId: 'c'));
    await Future<void>.delayed(Duration.zero);
    expect(session.cursor, 4);
    await session.disconnect();
    await session.connect();
    expect(fake.cursors.last, 4);
    await session.dispose();
  });

  test('sanitized_error_mapping', () {
    final failure = BackendFailure.fromNative(
      Exception('token=private /private/path'),
    );
    expect(failure.toString(), isNot(contains('private')));
    expect(failure.code, FailureCode.unavailableTransport);
  });

  test('unsubscribe_does_not_cancel_run', () async {
    final fake = FakeBackend();
    final session = BackendSession(fake);
    await session.connect();
    await session.disconnect();
    expect(fake.commands, isEmpty);
    expect(fake.closed, isFalse);
    await session.dispose();
    expect(fake.closed, isFalse);
  });

  testWidgets('no_native_library_in_widget_test', (tester) async {
    final fake = FakeBackend();
    await tester.pumpWidget(
      MaterialApp(
        home: FutureBuilder<AppSnapshot>(
          future: fake.snapshot(),
          builder: (context, state) =>
              Text(state.hasData ? 'ready' : 'loading'),
        ),
      ),
    );
    await tester.pump();
    expect(find.text('ready'), findsOneWidget);
  });

  test('engine_owner_shares_one_backend', () async {
    var opens = 0;
    final fake = FakeBackend();
    final owner = BackendOwner(() async {
      opens++;
      return fake;
    });
    final pair = await Future.wait([owner.backend, owner.backend]);
    expect(identical(pair.first, pair.last), isTrue);
    expect(opens, 1);
    await owner.close();
    expect(fake.closed, isTrue);
  });
}
