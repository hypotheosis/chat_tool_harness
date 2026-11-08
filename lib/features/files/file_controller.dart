import '../../backend/backend.dart';
import '../../backend/models.dart';

// Text is returned literally; Task 11 must render it without HTML or URL loading.
abstract interface class AndroidDocumentAccess {
  Future<DocumentResult> saveArtifact(ArtifactContent artifact);
  Future<DocumentResult> shareArtifact(ArtifactContent artifact);
}

enum DocumentResult { saved, shared, cancelled }

class FileController {
  FileController(this.backend, this.documents, this.conversationId);
  final Backend backend;
  final AndroidDocumentAccess documents;
  final String conversationId;
  bool _exporting = false;
  Future<List<ArtifactSummary>> list() async => (await backend.snapshot())
      .artifacts
      .where((a) => a.conversationId == conversationId)
      .toList();
  Future<ArtifactContent> _read(String id) async {
    final reply = await backend.execute(ReadArtifact(conversationId, id));
    return reply.artifact ?? (throw const BackendFailure(FailureCode.protocol));
  }

  Future<String> preview(String artifactId) async =>
      (await _read(artifactId)).text;
  Future<DocumentResult> save(String artifactId) =>
      _export(artifactId, documents.saveArtifact);
  Future<DocumentResult> share(String artifactId) =>
      _export(artifactId, documents.shareArtifact);
  Future<DocumentResult> _export(
    String id,
    Future<DocumentResult> Function(ArtifactContent) action,
  ) async {
    if (_exporting) throw const BackendFailure(FailureCode.conflict);
    _exporting = true;
    try {
      return await action(await _read(id));
    } finally {
      _exporting = false;
    }
  }

  Future<void> delete(String artifactId) async {
    await backend.execute(DeleteArtifact(conversationId, artifactId));
  }
}
