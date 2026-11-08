part of 'models.dart';

class ConversationSummary {
  const ConversationSummary({
    required this.id,
    required this.title,
    required this.hasUnknownOutcomes,
  });
  final String id;
  final String title;
  final bool hasUnknownOutcomes;
}

class ExportBundle {
  const ExportBundle({required this.transcript});
  final ArtifactSummary transcript;
}

class SearchConversations extends AppCommand {
  const SearchConversations(this.query, {this.offset = 0, this.limit = 100});
  final String query;
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

class ExportTranscript extends AppCommand {
  const ExportTranscript(
    this.conversationId, {
    this.includeToolPayloads = false,
  });
  final String conversationId;
  final bool includeToolPayloads;
}
