part of 'models.dart';

class ArtifactSummary {
  const ArtifactSummary({
    required this.id,
    required this.conversationId,
    required this.name,
    required this.bytes,
  });
  final String id;
  final String conversationId;
  final String name;
  final int bytes;
}

class ArtifactContent {
  const ArtifactContent({
    required this.summary,
    required this.text,
    required this.privatePath,
  });
  final ArtifactSummary summary;
  final String text;
  final String privatePath;
}

class CreateArtifact extends AppCommand {
  const CreateArtifact(this.conversationId, this.name, this.content);
  final String conversationId;
  final String name;
  final String content;
}

class ReadArtifact extends AppCommand {
  const ReadArtifact(this.conversationId, this.artifactId);
  final String conversationId;
  final String artifactId;
}

class DeleteArtifact extends AppCommand {
  const DeleteArtifact(this.conversationId, this.artifactId);
  final String conversationId;
  final String artifactId;
}
