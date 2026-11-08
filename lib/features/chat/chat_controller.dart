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

  Future<void> applyProfile(String id) async {
    await perform(ApplyProfile(conversationId, id));
  }

  Future<void> acknowledgeInferenceDisclosure(ProfileSnapshot profile) async {
    if (profile.inferenceDestination == null) {
      error = failureMessage(const BackendFailure(FailureCode.configuration));
      changed();
      return;
    }
    await perform(
      AcknowledgeContextDisclosure(
        conversationId,
        profile.revision,
        profile.inferenceDestination!,
      ),
    );
  }

  Future<void> select(String runId, ContextSelection selection) async {
    await perform(ApplyContextSelection(runId, selection));
  }

  Future<void> summarize(ContextSelection selection) async {
    await perform(SummarizeContext(conversationId, selection));
  }
}
