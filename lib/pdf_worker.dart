import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'src/rust/api/pdf.dart';
import 'src/rust/frb_generated.dart';
import 'src/rust/model.dart';

const _engine = MethodChannel('chat_tool_harness/pdf_worker_engine');

/// Retained, headless entrypoint in the disposable :pdf_worker process only.
@pragma('vm:entry-point')
Future<void> pdfWorkerMain() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  _engine.setMethodCallHandler((call) async {
    if (call.method != 'extract') throw MissingPluginException(call.method);
    final args = Map<String, dynamic>.from(call.arguments as Map);
    final fdIn = args['inputFd'];
    final fdOut = args['outputFd'];
    final id = args['requestId'];
    if (fdIn is! int || fdOut is! int || id is! String) {
      throw const FormatException('Invalid internal worker descriptor');
    }
    try {
      await extractPdfWorker(
        inputFdPath: '/proc/self/fd/$fdIn',
        outputFdPath: '/proc/self/fd/$fdOut',
        requestId: id,
        limits: PdfLimits(
          maxInputBytes: args['maxInputBytes'] as int,
          maxPages: args['maxPages'] as int,
          maxTextBytes: args['maxTextBytes'] as int,
          maxDecompressedBytes: args['maxDecompressedBytes'] as int,
        ),
      );
    } finally {
      await _engine.invokeMethod<void>('finished');
    }
  });
  await _engine.invokeMethod<void>('ready');
}
