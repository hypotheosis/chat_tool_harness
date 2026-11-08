import '../../backend/models.dart';
import '../screen_controller.dart';

class ChatController extends ScreenController {
  ChatController(super.backend, this.conversationId);
  final String conversationId;
  Conversation? get conversation {
    for (final c in snapshot?.conversations ?? <Conversation>[]) {
      if (c.id == conversationId) return c;
    }
    return null;
  }

  List<Run> get runs => [
    for (final r in snapshot?.runs ?? <Run>[])
      if (r.conversationId == conversationId) r,
  ];
  Future<void> send(String text) async {
    await perform(StartTurn(conversationId, text));
  }

  Future<void> approve(ApprovalBinding binding) async {
    await perform(Approve(binding), approval: true);
  }

  Future<void> deny(String id) async {
    await perform(Deny(id), approval: true);
  }

  Future<void> stop(String runId) async {
    await perform(CancelRun(runId));
  }

  Future<void> resume(String runId) async {
    await perform(ResumeRun(runId));
  }

  /// Acknowledges disclosure for [origin]; true when the backend accepted it.
  Future<bool> acknowledgeOrigin(String origin) async =>
      await perform(AcknowledgeContextDisclosure(conversationId, origin)) !=
      null;

  /// Acknowledges the paused run's own origin, then resumes it.
  Future<void> allowAndResume(Run run) async {
    final origin = run.profile.inferenceOrigin;
    if (origin == null) {
      reject(failureMessage(const BackendFailure(FailureCode.configuration)));
      return;
    }
    if (await acknowledgeOrigin(origin)) await resume(run.id);
  }

  Future<void> select(String runId, ContextSelection selection) async {
    await perform(ApplyContextSelection(runId, selection));
  }

  Future<void> summarize(ContextSelection selection) async {
    await perform(SummarizeContext(conversationId, selection));
  }
}
