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

  /// Approves [approve] and denies [deny] (attempt ids) of run [runId] in one
  /// command, which then resumes the run.
  Future<void> decide(
    String runId, {
    List<ApprovalBinding> approve = const [],
    List<String> deny = const [],
  }) async {
    await perform(
      DecideApprovals(runId, approve: approve, deny: deny),
      approval: true,
    );
  }

  /// Trusts [serverId], then approves [approve] of run [runId]; approves
  /// nothing when trusting fails.
  Future<void> trustAndApprove(
    String runId,
    String serverId,
    List<ApprovalBinding> approve,
  ) async {
    final trusted = await perform(SetServerTrust(serverId, trusted: true));
    if (trusted != null) await decide(runId, approve: approve);
  }

  Future<void> stop(String runId) async {
    await perform(CancelRun(runId));
  }

  Future<void> resume(String runId) async {
    await perform(ResumeRun(runId));
  }

  /// Returns this archived conversation to the Conversations list, so it
  /// accepts new work again.
  Future<void> restore() async {
    await perform(RestoreConversation(conversationId));
  }

  /// Re-runs failed turn [runId] from its own input message, adding none.
  Future<void> retry(String runId) async {
    await perform(RetryRun(runId));
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
