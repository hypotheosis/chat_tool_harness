import 'dart:async';
import 'package:chat_tool_harness/backend/backend.dart';
import 'package:chat_tool_harness/backend/models.dart';

class FakeBackend implements Backend {
  AppSnapshot current = const AppSnapshot();
  int snapshotCalls = 0;
  final snapshotResponses = <Future<AppSnapshot>>[];
  bool closed = false;
  final commands = <AppCommand>[];
  final cursors = <int>[];
  final _events = StreamController<AppEvent>.broadcast(sync: true);
  BackendFailure? failure;
  void emit(AppEvent event) {
    if (event.sequence > current.sequence) {
      current = AppSnapshot(
        sequence: event.sequence,
        endpoints: current.endpoints,
        servers: current.servers,
        profiles: current.profiles,
        conversations: current.conversations,
        runs: current.runs,
        rules: current.rules,
        connectionTests: current.connectionTests,
      );
    }
    _events.add(event);
  }

  @override
  Future<CommandReply> execute(AppCommand command) async {
    commands.add(command);
    if (failure case final error?) throw error;
    return const CommandReply();
  }

  @override
  Future<AppSnapshot> snapshot() async {
    snapshotCalls++;
    if (snapshotResponses.isNotEmpty) return snapshotResponses.removeAt(0);
    return current;
  }

  @override
  Stream<AppEvent> events(int afterSequence) {
    cursors.add(afterSequence);
    return _events.stream.where((e) => e.sequence > afterSequence);
  }

  @override
  Future<void> close() async {
    closed = true;
    await _events.close();
  }
}
