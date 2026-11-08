import 'dart:async';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'backend.dart';
import 'models.dart';

/// Engine-owned subscription, independent of Activity/widget attachment.
class EngineLifecycle {
  EngineLifecycle(this.backend);
  final Future<Backend> Function() backend;
  final openConversation = ValueNotifier<String?>(null);
  final notice = ValueNotifier<String?>(null);
  StreamSubscription<dynamic>? _subscription;
  Future<void> _queue = Future.value();
  static const channel = EventChannel('chat_tool_harness/agent_lifecycle');
  static final events = channel.receiveBroadcastStream().asBroadcastStream();
  void initialize() {
    _subscription ??= events.listen(
      (event) {
        _queue = _queue.then((_) => handle(event)).catchError((Object _) {
          notice.value =
              'Run status unavailable. Open the app and review before explicitly resuming. Stop is not remote rollback.';
        });
      },
      onError: (Object _) {
        notice.value =
            'Background host unavailable. Review the run before resuming.';
      },
    );
  }

  Future<void> handle(dynamic event) async {
    if (event is! Map) return;
    final runId = event['runId'];
    final conversationId = event['conversationId'];
    final action = event['action'];
    if (runId is! String || conversationId is! String) return;
    final core = await backend();
    final snapshot = await core.snapshot();
    final runs = snapshot.runs.where(
      (r) => r.id == runId && r.conversationId == conversationId,
    );
    if (runs.isEmpty) return;
    if (action == 'open') {
      openConversation.value = conversationId;
    } else if (runs.single.state == RunState.running) {
      final generation = event['lease'];
      if (generation is! String || generation.isEmpty) return;
      // Snapshot state is presentation only. Core atomically admits exact-generation actions.
      try {
        if (action == 'stop') {
          await core.execute(PlatformStop(runId, generation));
          notice.value =
              'Stopped locally. Already-sent tool effects may be unknown; Stop is not rollback.';
        } else if (const ['timeout', 'refused', 'hostLost'].contains(action)) {
          await core.execute(PlatformPause(runId, generation));
          notice.value =
              'Background host interrupted. Review paused/unknown work; explicitly resume only when safe.';
        }
      } on BackendFailure catch (failure) {
        if (failure.code != FailureCode.conflict) rethrow;
      }
    }
  }

  Future<void> close() async {
    await _subscription?.cancel();
    await _queue;
  }
}
