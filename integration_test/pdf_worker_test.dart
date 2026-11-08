import 'package:chat_tool_harness/main.dart' as app;
import 'package:chat_tool_harness/pdf_worker_host.dart';
import 'package:chat_tool_harness/src/rust/model.dart';
import 'package:chat_tool_harness/src/rust/error.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  const runtime = MethodChannel('chat_tool_harness/agent_runtime');
  const probe = MethodChannel('chat_tool_harness/agent_probe');
  const id = '123e4567-e89b-12d3-a456-426614174001';
  const limits = PdfLimits(
    maxInputBytes: 10 * 1024 * 1024,
    maxPages: 500,
    maxTextBytes: 2 * 1024 * 1024,
    maxDecompressedBytes: 4 * 1024 * 1024,
  );

  testWidgets(
    'pdf_worker_is_separate_and_disposable',
    (tester) async {
      await app.main();
      await tester.pumpAndSettle();
      final before = await runtime.invokeMapMethod<String, dynamic>('state');
      final fixture = await probe.invokeMethod<String>(
        'pdfFixture',
        'two_pages',
      );
      final reply = await const PdfWorkerHost().extract(
        PdfWorkerRequest(requestId: id, snapshotPath: fixture!, limits: limits),
      );
      expect(reply, isA<PdfWorkerReply_Extracted>());
      final result = (reply as PdfWorkerReply_Extracted).field0;
      expect(result.extraction.pages.map((p) => p.physicalPage), [1, 2]);
      expect(result.extraction.pages.first.text, contains('First'));
      final diagnostics = await probe.invokeMapMethod<String, dynamic>(
        'pdfDiagnostics',
      );
      expect(diagnostics?['workerPid'], isNot(before?['pid']));
      expect(diagnostics?['workerRole'], 'pdf_worker');
      expect(diagnostics?['workerChatInitialized'], false);
      expect(
        (await runtime.invokeMapMethod<String, dynamic>('state'))?['runtimeId'],
        before?['runtimeId'],
      );

      final encrypted = await probe.invokeMethod<String>(
        'pdfFixture',
        'encrypted_empty_password',
      );
      final encryptedReply = await const PdfWorkerHost().extract(
        PdfWorkerRequest(
          requestId: '123e4567-e89b-12d3-a456-426614174002',
          snapshotPath: encrypted!,
          limits: limits,
        ),
      );
      expect(
        encryptedReply,
        const PdfWorkerReply.failed(PdfFailure.unsupportedEncrypted),
      );

      await probe.invokeMethod<void>('pdfFault', 'abort');
      final dead = await const PdfWorkerHost().extract(
        PdfWorkerRequest(
          requestId: '123e4567-e89b-12d3-a456-426614174003',
          snapshotPath: fixture,
          limits: limits,
        ),
      );
      final afterAbort = await probe.invokeMapMethod<String, dynamic>(
        'pdfDiagnostics',
      );
      expect(
        dead,
        const PdfWorkerReply.failed(PdfFailure.workerDied),
        reason: '$afterAbort',
      );
      expect(
        (await runtime.invokeMapMethod<String, dynamic>('state'))?['runtimeId'],
        before?['runtimeId'],
      );
      await probe.invokeMethod<void>('pdfFault', 'partial');
      final partial = await const PdfWorkerHost().extract(
        PdfWorkerRequest(
          requestId: '123e4567-e89b-12d3-a456-426614174005',
          snapshotPath: fixture,
          limits: limits,
        ),
      );
      expect(partial, const PdfWorkerReply.failed(PdfFailure.protocol));
      final retry = await const PdfWorkerHost().extract(
        PdfWorkerRequest(
          requestId: '123e4567-e89b-12d3-a456-426614174004',
          snapshotPath: fixture,
          limits: limits,
        ),
      );
      expect(retry, isA<PdfWorkerReply_Extracted>());

      await probe.invokeMethod<void>('pdfFault', 'withhold');
      final pending = const PdfWorkerHost().extract(
        PdfWorkerRequest(
          requestId: '123e4567-e89b-12d3-a456-426614174006',
          snapshotPath: fixture,
          limits: limits,
        ),
      );
      final cancel = await const PdfWorkerHost().cancel(
        '123e4567-e89b-12d3-a456-426614174006',
      );
      expect(cancel, const PlatformCancelReply.ok());
      expect(await pending, const PdfWorkerReply.failed(PdfFailure.cancelled));

      await probe.invokeMethod<void>('pdfFault', 'withhold');
      final elapsed = Stopwatch()..start();
      final deadlineReply = await const PdfWorkerHost().extract(
        PdfWorkerRequest(
          requestId: '123e4567-e89b-12d3-a456-426614174060',
          snapshotPath: fixture,
          limits: limits,
        ),
      );
      expect(deadlineReply, const PdfWorkerReply.failed(PdfFailure.timeout));
      expect(
        elapsed.elapsed,
        greaterThanOrEqualTo(const Duration(seconds: 60)),
      );
      expect(elapsed.elapsed, lessThan(const Duration(seconds: 70)));
      final deadlineDiagnostics = await probe.invokeMapMethod<String, dynamic>(
        'pdfDiagnostics',
      );
      expect(deadlineDiagnostics?['lastDeath'], 'binderDeath');
      expect(deadlineDiagnostics?['unavailable'], false);
      expect(
        (await runtime.invokeMapMethod<String, dynamic>('state'))?['runtimeId'],
        before?['runtimeId'],
      );
      final deadlineRetry = await const PdfWorkerHost().extract(
        PdfWorkerRequest(
          requestId: '123e4567-e89b-12d3-a456-426614174061',
          snapshotPath: fixture,
          limits: limits,
        ),
      );
      expect(deadlineRetry, isA<PdfWorkerReply_Extracted>());
    },
    timeout: const Timeout(Duration(seconds: 100)),
  );
}
