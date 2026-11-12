part of 'models.dart';

class ConversationSummary {
  const ConversationSummary({
    required this.id,
    required this.title,
    required this.hasUnknownOutcomes,
    this.archived = false,
  });
  final String id;
  final String title;
  final bool hasUnknownOutcomes;

  /// Hidden from the Conversations list; restore it to start new work.
  final bool archived;
}

class ExportBundle {
  const ExportBundle({required this.transcript});
  final ArtifactSummary transcript;
}

/// Archived conversations are included and marked; [archivedOnly] keeps just
/// those.
class SearchConversations extends AppCommand {
  const SearchConversations(
    this.query, {
    this.archivedOnly = false,
    this.offset = 0,
    this.limit = 100,
  });
  final String query;
  final bool archivedOnly;
  final int offset;
  final int limit;
}

class RenameConversation extends AppCommand {
  const RenameConversation(this.conversationId, this.title);
  final String conversationId;
  final String title;
}

class DeleteConversation extends AppCommand {
  const DeleteConversation(
    this.conversationId, {
    required this.confirmedUnknown,
  });
  final String conversationId;
  final bool confirmedUnknown;
}

/// Hides a conversation from the Conversations list, keeping its history.
/// The backend refuses it while the conversation has active work.
class ArchiveConversation extends AppCommand {
  const ArchiveConversation(this.conversationId);
  final String conversationId;
}

/// Returns an archived conversation to the Conversations list.
class RestoreConversation extends AppCommand {
  const RestoreConversation(this.conversationId);
  final String conversationId;
}

class ExportTranscript extends AppCommand {
  const ExportTranscript(
    this.conversationId, {
    this.includeToolPayloads = false,
  });
  final String conversationId;
  final bool includeToolPayloads;
}
