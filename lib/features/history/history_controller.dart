import 'dart:async';
import 'package:flutter/foundation.dart';
import '../../backend/backend.dart';
import '../../backend/models.dart';
import '../files/file_controller.dart';
import '../screen_controller.dart';

class HistoryController extends ChangeNotifier {
  HistoryController(Backend backend, {this.documents})
    : session = BackendSession(backend);
  final BackendSession session;
  Backend get backend => session.backend;
  final AndroidDocumentAccess? documents;
  List<ConversationSummary> rows = [];
  String query = '';

  /// Whether the list shows only archived conversations.
  bool archivedOnly = false;
  int offset = 0;
  bool loading = false;
  bool get hasNext => rows.length == 100;
  String? error;
  ArtifactContent? transcript;
  String? delivery;
  StreamSubscription<AppSnapshot>? _subscription;
  bool _disposed = false;
  int _searchGeneration = 0;
  // A change seen while busy; refreshed once when the current action ends.
  bool _refreshPending = false;
  // The session sequence the latest search started from; it covers changes
  // up to here.
  int _searchedThrough = -1;
  void changed() {
    if (!_disposed) notifyListeners();
  }

  Future<void> connect() async {
    await _action(() async {
      await session.connect();
      _subscription = session.changes.listen(
        (snapshot) {
          if (snapshot.sequence <= _searchedThrough) return;
          if (loading) {
            _refreshPending = true;
          } else {
            unawaited(search(query, pageOffset: offset));
          }
        },
        onError: (Object e) {
          error = failureMessage(e);
          changed();
        },
      );
      await _search();
    });
  }

  Future<void> _search() async {
    // This search observes every change seen so far.
    _refreshPending = false;
    _searchedThrough = session.cursor;
    final generation = ++_searchGeneration;
    final reply = await backend.execute(
      SearchConversations(query, archivedOnly: archivedOnly, offset: offset),
    );
    if (!_disposed && generation == _searchGeneration) rows = reply.history;
  }

  Future<void> search(String value, {int pageOffset = 0}) async {
    if (loading || _disposed) return;
    query = value;
    offset = pageOffset;
    await _action(_search);
  }

  /// Switches between all conversations and only archived ones, from the
  /// first page.
  Future<void> showArchived(bool value) async {
    if (loading || _disposed) return;
    archivedOnly = value;
    await search(query);
  }

  Future<void> nextPage() => search(query, pageOffset: offset + 100);
  Future<void> previousPage() =>
      search(query, pageOffset: offset >= 100 ? offset - 100 : 0);
  Future<void> rename(String owner, String title) =>
      _mutate(RenameConversation(owner, title));
  Future<void> delete(String owner, {required bool confirmedUnknown}) =>
      _mutate(DeleteConversation(owner, confirmedUnknown: confirmedUnknown));
  Future<void> archive(String owner) => _mutate(ArchiveConversation(owner));
  Future<void> restore(String owner) => _mutate(RestoreConversation(owner));
  Future<void> _mutate(AppCommand command) => _action(() async {
    await session.execute(command);
    await _search();
  });
  List<Run> runs(String owner) => (session.current?.runs ?? <Run>[])
      .where((r) => r.conversationId == owner)
      .toList();
  bool needsStop(String owner) => runs(owner).any(
    (r) => r.state == RunState.running || r.state == RunState.waitingApproval,
  );
  Future<void> stop(String owner) => _action(() async {
    // CancelRun waits for the Core's local execution/host teardown, never a remote rollback.
    for (final run in runs(owner).where(
      (r) => r.state == RunState.running || r.state == RunState.waitingApproval,
    )) {
      await session.execute(CancelRun(run.id));
    }
    await _search();
  });
  void clearPreview() {
    transcript = null;
    delivery = null;
    changed();
  }

  Future<void> preview(String owner, bool details) => _action(() async {
    clearPreview();
    final reply = await session.execute(
      ExportTranscript(owner, includeToolPayloads: details),
    );
    final summary = reply.exportBundle?.transcript;
    if (summary == null) throw const BackendFailure(FailureCode.protocol);
    final read = await backend.execute(ReadArtifact(owner, summary.id));
    if (read.artifact == null) throw const BackendFailure(FailureCode.protocol);
    if (!_disposed) transcript = read.artifact;
  });
  Future<void> deliver(bool share) => _action(() async {
    final a = transcript;
    final d = documents;
    if (a == null || d == null) {
      throw const BackendFailure(FailureCode.invalidInput);
    }
    final files = FileController(backend, d, a.summary.conversationId);
    final result = await (share
        ? files.share(a.summary.id)
        : files.save(a.summary.id));
    delivery = switch (result) {
      DocumentResult.saved => 'Saved',
      DocumentResult.shared => 'Shared',
      DocumentResult.cancelled => 'Cancelled',
    };
  });
  Future<void> _action(
    Future<void> Function() action, {
    bool keepError = false,
  }) async {
    if (loading || _disposed) return;
    loading = true;
    if (!keepError) error = null;
    changed();
    try {
      await action();
    } catch (e) {
      error =
          e is BackendFailure &&
              (e.code == FailureCode.authentication ||
                  e.code == FailureCode.credentialMissing)
          ? 'Export blocked: configured credential missing or unavailable. Repair credentials in Settings; history browsing is still available.'
          : e is BackendFailure && e.code == FailureCode.limit
          ? 'History/export safety limit reached: transcripts must fit 1 MiB and the private storage quota. Nothing was truncated; history remains available.'
          : failureMessage(e);
    } finally {
      loading = false;
      changed();
    }
    // Keep the finished action's error visible through the follow-up refresh.
    if (_refreshPending && !_disposed) {
      unawaited(_action(_search, keepError: true));
    }
  }

  @override
  void dispose() {
    _disposed = true;
    _searchGeneration++;
    unawaited(_subscription?.cancel());
    unawaited(session.dispose());
    super.dispose();
  }
}
