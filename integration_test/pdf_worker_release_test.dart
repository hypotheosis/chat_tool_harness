import 'dart:convert';

import 'package:chat_tool_harness/main.dart' as app;
import 'package:chat_tool_harness/pdf_worker_host.dart';
import 'package:chat_tool_harness/src/rust/model.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

// The fixture is compiled only into this integration test's Dart target.
// No debug probe, fault switch, or production asset is used.
const _fixture =
    'JVBERi0xLjQKJeLjz9MKMSAwIG9iago8PCAvVHlwZSAvQ2F0YWxvZyAvUGFnZXMgMiAwIFIgPj4KZW5kb2JqCjIgMCBvYmoKPDwgL1R5cGUgL1BhZ2VzIC9LaWRzIFsgNSAwIFIgNyAwIFIgXSAvQ291bnQgMiA+PgplbmRvYmoKMyAwIG9iago8PCAvVHlwZSAvRm9udCAvU3VidHlwZSAvVHlwZTEgL0Jhc2VGb250IC9IZWx2ZXRpY2EgPj4KZW5kb2JqCjQgMCBvYmoKPDwgL0xlbmd0aCAzNiA+PgpzdHJlYW0KQlQgL0YxIDEyIFRmIDcyIDcyMCBUZCAoRmlyc3QpIFRqIEVUCmVuZHN0cmVhbQplbmRvYmoKNSAwIG9iago8PCAvVHlwZSAvUGFnZSAvUGFyZW50IDIgMCBSIC9NZWRpYUJveCBbMCAwIDYxMiA3OTJdIC9SZXNvdXJjZXMgPDwgL0ZvbnQgPDwgL0YxIDMgMCBSID4+ID4+IC9Db250ZW50cyA0IDAgUiA+PgplbmRvYmoKNiAwIG9iago8PCAvTGVuZ3RoIDM3ID4+CnN0cmVhbQpCVCAvRjEgMTIgVGYgNzIgNzIwIFRkIChTZWNvbmQpIFRqIEVUCmVuZHN0cmVhbQplbmRvYmoKNyAwIG9iago8PCAvVHlwZSAvUGFnZSAvUGFyZW50IDIgMCBSIC9NZWRpYUJveCBbMCAwIDYxMiA3OTJdIC9SZXNvdXJjZXMgPDwgL0ZvbnQgPDwgL0YxIDMgMCBSID4+ID4+IC9Db250ZW50cyA2IDAgUiA+PgplbmRvYmoKeHJlZgowIDgKMDAwMDAwMDAwMCA2NTUzNSBmIAowMDAwMDAwMDE1IDAwMDAwIG4gCjAwMDAwMDAwNjQgMDAwMDAgbiAKMDAwMDAwMDEyOSAwMDAwMCBuIAowMDAwMDAwMTk5IDAwMDAwIG4gCjAwMDAwMDAyODUgMDAwMDAgbiAKMDAwMDAwMDQxMSAwMDAwMCBuIAowMDAwMDAwNDk4IDAwMDAwIG4gCnRyYWlsZXIKPDwgL1NpemUgOCAvUm9vdCAxIDAgUiA+PgpzdGFydHhyZWYKNjI0CiUlRU9GCg==';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets('release worker AOT extracts two real pages', (tester) async {
    await app.main();
    await tester.pumpAndSettle();
    final path = await const MethodChannel(
      'chat_tool_harness/pdf_snapshot',
    ).invokeMethod<String>('stage', base64Decode(_fixture));
    final reply = await const PdfWorkerHost().extract(
      PdfWorkerRequest(
        requestId: '123e4567-e89b-12d3-a456-426614174abc',
        snapshotPath: path!,
        limits: const PdfLimits(
          maxInputBytes: 10 * 1024 * 1024,
          maxPages: 500,
          maxTextBytes: 2 * 1024 * 1024,
          maxDecompressedBytes: 4 * 1024 * 1024,
        ),
      ),
    );
    expect(reply, isA<PdfWorkerReply_Extracted>());
    final pages = (reply as PdfWorkerReply_Extracted).field0.extraction.pages;
    expect(pages.map((page) => page.physicalPage), [1, 2]);
    expect(pages.first.text, contains('First'));
    expect(pages.last.text, contains('Second'));
  });
}
