import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/app.dart';
import 'package:chat_tool_harness/backend/models.dart';
import 'support/ui_fixture.dart';

class FailOnceSnapshot extends UiBackend {
  bool first = true;
  @override
  Future<AppSnapshot> snapshot() async {
    if (first) {
      first = false;
      throw const BackendFailure(FailureCode.unavailableTransport);
    }
    return super.snapshot();
  }
}

void main() {
  testWidgets('refresh_after_initial_failure_reconnects_events', (t) async {
    final b = FailOnceSnapshot();
    await t.pumpWidget(AgentApp(backend: b));
    await t.pumpAndSettle();
    expect(find.textContaining('unavailable'), findsOneWidget);
    await t.tap(find.text('Refresh'));
    await t.pumpAndSettle();
    expect(b.cursors, hasLength(1));
    expect(find.textContaining('Old profile'), findsOneWidget);
  });
  testWidgets('app_boot_is_native_free_with_borrowed_backend', (t) async {
    final b = UiBackend();
    await t.pumpWidget(AgentApp(backend: b));
    await t.pumpAndSettle();
    expect(find.text('Conversations'), findsOneWidget);
    expect(find.textContaining('Old profile'), findsOneWidget);
    await t.tap(find.text('New conversation'));
    await t.pumpAndSettle();
    await t.tap(find.text('Profile • p'));
    await t.pumpAndSettle();
    expect(b.commands.whereType<CreateConversation>().single.profileId, 'p');
    await t.pumpWidget(const SizedBox());
    await t.pumpAndSettle();
    expect(b.closed, isFalse);
  });
  testWidgets('home_and_settings_show_no_snapshot_wording', (t) async {
    final b = UiBackend();
    await t.pumpWidget(AgentApp(backend: b));
    await t.pumpAndSettle();
    expect(find.textContaining('Old profile'), findsOneWidget);
    expect(find.textContaining('revision'), findsNothing);
    expect(find.textContaining('Snapshot'), findsNothing);
    await t.tap(find.byTooltip('Settings'));
    await t.pumpAndSettle();
    expect(
      find.text(
        'Reusable configurations. Conversations pick up saved settings on their next turn.',
      ),
      findsOneWidget,
    );
    expect(find.textContaining('snapshot'), findsNothing);
    expect(find.textContaining('Apply current profile'), findsNothing);
  });
  testWidgets('empty_home_routes_to_configuration', (t) async {
    final b = UiBackend()..current = const AppSnapshot();
    await t.pumpWidget(AgentApp(backend: b));
    await t.pumpAndSettle();
    expect(find.textContaining('No conversations'), findsOneWidget);
    await t.tap(find.byTooltip('Settings'));
    await t.pumpAndSettle();
    expect(find.text('Add endpoint'), findsOneWidget);
  });
  testWidgets('home_leaves_out_archived_conversations', (t) async {
    final b = UiBackend()..setConversation(archived: true);
    await t.pumpWidget(AgentApp(backend: b));
    await t.pumpAndSettle();
    expect(find.textContaining('Old profile'), findsNothing);
    expect(
      find.textContaining('All conversations are archived'),
      findsOneWidget,
    );
    expect(find.textContaining('No conversations'), findsNothing);
    b.setConversation(archived: false);
    b.emit(const AppEvent(sequence: 1, conversationId: 'c'));
    await t.pumpAndSettle();
    expect(find.textContaining('Old profile'), findsOneWidget);
    expect(find.textContaining('All conversations are archived'), findsNothing);
    await t.pumpWidget(const SizedBox());
  });
}
