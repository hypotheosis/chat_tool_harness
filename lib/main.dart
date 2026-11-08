import 'package:flutter/material.dart';
import 'backend/platform_services.dart';
import 'app.dart';
import 'features/files/android_document_access.dart';
import 'package:chat_tool_harness/src/rust/frb_generated.dart';
// Retain the separate worker library in the app's compiled Dart program.
import 'package:chat_tool_harness/pdf_worker.dart' as worker;

final _workerEntrypointAnchor = worker.pdfWorkerMain;

Future<void> main() async {
  // Reachability keeps the worker entrypoint in debug and AOT snapshots.
  _workerEntrypointAnchor.hashCode;
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  await EngineBackend.initialize();
  runApp(
    AgentApp(
      backend: await EngineBackend.owner.backend,
      documents: const MethodChannelDocumentAccess(),
      lifecycle: EngineBackend.lifecycle,
    ),
  );
}
