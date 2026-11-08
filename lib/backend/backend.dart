import 'dart:async';
import 'models.dart';

abstract interface class Backend {
  Future<CommandReply> execute(AppCommand command);
  Future<AppSnapshot> snapshot();
  Stream<AppEvent> events(int afterSequence);
  Future<void> close();
}

/// Engine ownership is independent of any widget or event subscription.
class BackendOwner {
  BackendOwner(this._open);
  final Future<Backend> Function() _open;
  Future<Backend>? _backend;
  Future<void>? _closing;
  Future<Backend> get backend {
    if (_closing != null) {
      return Future.error(const BackendFailure(FailureCode.conflict));
    }
    return _backend ??= _open();
  }

  Future<void> close() => _closing ??= _close();
  Future<void> _close() async {
    final pending = _backend;
    if (pending != null) await (await pending).close();
  }
}

/// Presentation cache only; all decisions and state transitions remain in Rust.
class BackendSession {
  BackendSession(this.backend);
  final Backend backend;
  final _changes = StreamController<AppSnapshot>.broadcast();
  Stream<AppSnapshot> get changes => _changes.stream;
  AppSnapshot? current;
  int get cursor => _cursor;
  int _cursor = 0;
  int _generation = 0;
  int _refreshGeneration = 0;
  StreamSubscription<AppEvent>? _subscription;
  Future<void> _ordered = Future.value();

  Future<void> connect() async {
    await disconnect();
    final generation = _generation;
    await refresh();
    if (generation != _generation) return;
    _subscription = backend
        .events(_cursor)
        .listen(
          (event) {
            _ordered = _ordered
                .then((_) async {
                  if (generation != _generation || event.sequence <= _cursor) {
                    return;
                  }
                  if (event.sequence != _cursor + 1) {
                    await refresh();
                    if (event.sequence <= _cursor) return;
                  }
                  // Events are durable invalidations, not independently mutable UI state.
                  await refresh();
                })
                .catchError((Object error, StackTrace stack) {
                  if (!_changes.isClosed) _changes.addError(error, stack);
                });
          },
          onError: (Object error, StackTrace stack) {
            if (!_changes.isClosed) _changes.addError(error, stack);
          },
        );
  }

  Future<void> refresh() async {
    final generation = ++_refreshGeneration;
    final snapshot = await backend.snapshot();
    if (generation != _refreshGeneration || snapshot.sequence < _cursor) return;
    current = snapshot;
    _cursor = snapshot.sequence;
    if (!_changes.isClosed) _changes.add(snapshot);
  }

  Future<CommandReply> execute(AppCommand command) async {
    final result = await backend.execute(command);
    await refresh();
    return result;
  }

  Future<void> disconnect() async {
    _generation++;
    _refreshGeneration++;
    await _subscription?.cancel();
    _subscription = null;
    await _ordered;
  }

  Future<void> dispose() async {
    await disconnect();
    await _changes.close();
  }
}
