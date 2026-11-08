import 'dart:async';
import 'dart:math' as math;
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/frb_backend.dart';
import 'package:chat_tool_harness/backend/platform_services.dart';
import 'package:chat_tool_harness/src/rust/frb_generated.dart';
import 'package:chat_tool_harness/src/rust/api/agent.dart' as api;
import 'package:chat_tool_harness/src/rust/application.dart' as callbacks;
import 'package:chat_tool_harness/src/rust/application/dto.dart' as dto;
import 'package:chat_tool_harness/src/rust/model.dart' as platform;

class _Handle implements api.AgentHandle {
  @override
  void dispose() {}
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _ReplayApi implements RustLibApi {
  int polls = 0;
  int closes = 0;
  @override
  Future<api.AgentHandle> crateApiAgentOpenAgent({
    required String dataDir,
    required FutureOr<callbacks.CredentialReply> Function(String)
    resolveCredential,
    required FutureOr<platform.PlatformReply> Function(platform.RunHostRequest)
    setServiceActive,
  }) async => _Handle();
  @override
  Stream<dto.AgentAppEvent> crateApiAgentAgentEvents({
    required api.AgentHandle handle,
    required BigInt afterSequence,
  }) {
    polls++;
    return const Stream.empty();
  }

  @override
  Future<void> crateApiAgentCloseAgent({
    required api.AgentHandle handle,
  }) async {
    closes++;
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _TrackedTimer implements Timer {
  _TrackedTimer(this.active);
  final Set<_TrackedTimer> active;
  late void Function() callback;
  bool _active = true;
  void fire() {
    if (!_active) return;
    _active = false;
    active.remove(this);
    callback();
  }

  @override
  void cancel() {
    _active = false;
    active.remove(this);
  }

  @override
  bool get isActive => _active;
  @override
  int get tick => _active ? 0 : 1;
}

void main() {
  for (final close in [false, true]) {
    test(
      'many_idle_polls_cleanup_on_${close ? 'close' : 'unsubscribe'}',
      () async {
        final mock = _ReplayApi();
        RustLib.initMock(api: mock);
        addTearDown(RustLib.dispose);
        final active = <_TrackedTimer>{};
        var maximum = 0;
        var registrations = 0;
        final backend = await FrbBackend.open(
          'unused',
          PlatformServices(
            resolveCredential: (_) async => null,
            setServiceActive: (_) async {},
          ),
        );
        late StreamSubscription<Object?> subscription;
        final done = Completer<void>();
        runZoned(
          () {
            subscription = backend
                .events(0)
                .listen((_) {}, onDone: done.complete);
          },
          zoneSpecification: ZoneSpecification(
            createTimer: (self, parent, zone, duration, callback) {
              final timer = _TrackedTimer(active);
              active.add(timer);
              registrations++;
              maximum = math.max(maximum, active.length);
              timer.callback = zone.bindCallbackGuarded(callback);
              return timer;
            },
          ),
        );
        await Future<void>.delayed(Duration.zero);
        for (var i = 0; i < 1024; i++) {
          active.single.fire();
          await Future<void>.delayed(Duration.zero);
        }
        expect(mock.polls, 1025);
        expect(registrations, 1025);
        expect(maximum, 1);
        if (close) {
          await backend.close();
        } else {
          await subscription.cancel();
        }
        await Future<void>.delayed(Duration.zero);
        expect(
          active,
          isEmpty,
          reason: 'teardown must cancel the current wait immediately',
        );
        final stoppedAt = mock.polls;
        await Future<void>.delayed(Duration.zero);
        expect(mock.polls, stoppedAt);
        if (close) {
          expect(done.isCompleted, isTrue);
        } else {
          expect(mock.closes, 0);
          await backend.close();
        }
        await subscription.cancel();
      },
    );
  }
}
