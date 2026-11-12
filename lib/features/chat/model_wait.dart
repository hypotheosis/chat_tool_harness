import 'dart:async';
import 'package:flutter/material.dart';
import '../../backend/models.dart';

/// Silence after which the wait says nothing has arrived, a little over
/// llama-server's 30 s keepalive interval.
const silenceNoticeMs = 45000;

/// `m:ss`, or `h:mm:ss` from an hour.
String waitClock(int ms) {
  final seconds = ms ~/ 1000;
  final h = seconds ~/ 3600;
  final m = seconds ~/ 60 % 60;
  final s = (seconds % 60).toString().padLeft(2, '0');
  return h > 0 ? '$h:${m.toString().padLeft(2, '0')}:$s' : '$m:$s';
}

/// The line under the wait: silence beats keepalives, and a fresh request
/// with neither says nothing.
String? waitNote(ModelWait wait, {int extraMs = 0}) {
  final silent = wait.silentMs + extraMs;
  if (silent >= silenceNoticeMs) {
    return 'Nothing received for ${silent ~/ 1000} s.';
  }
  if (wait.keepalives > 0) return 'The model is still processing the prompt.';
  return null;
}

/// "Waiting for the model… m:ss" for a run whose request has no output yet,
/// counting on each second from the snapshot's [wait] until a new one comes.
class ModelWaitStatus extends StatefulWidget {
  const ModelWaitStatus(this.wait, {super.key});
  final ModelWait wait;
  @override
  State<ModelWaitStatus> createState() => _ModelWaitStatusState();
}

class _ModelWaitStatusState extends State<ModelWaitStatus> {
  int ticks = 0;
  late final Timer timer;

  @override
  void initState() {
    super.initState();
    timer = Timer.periodic(
      const Duration(seconds: 1),
      (_) => setState(() => ticks++),
    );
  }

  @override
  void didUpdateWidget(ModelWaitStatus old) {
    super.didUpdateWidget(old);
    if (!identical(old.wait, widget.wait)) ticks = 0;
  }

  @override
  void dispose() {
    timer.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final extra = ticks * 1000;
    final note = waitNote(widget.wait, extraMs: extra);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'Waiting for the model… ${waitClock(widget.wait.waitedMs + extra)}',
        ),
        if (note != null)
          Text(
            note,
            style: Theme.of(context).textTheme.bodySmall?.copyWith(
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            ),
          ),
      ],
    );
  }
}
