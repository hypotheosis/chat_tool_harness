import 'package:chat_tool_harness/pdf_worker_host.dart';
import 'package:chat_tool_harness/src/rust/model.dart';
import 'package:chat_tool_harness/src/rust/error.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const channel = MethodChannel('chat_tool_harness/pdf_worker');
  const id = '123e4567-e89b-12d3-a456-426614174000';
  const limits = PdfLimits(
    maxInputBytes: 10 * 1024 * 1024,
    maxPages: 500,
    maxTextBytes: 2 * 1024 * 1024,
    maxDecompressedBytes: 4 * 1024 * 1024,
  );

  tearDown(
    () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, null),
  );

  test(
    'host sends only fixed request fields and maps typed page result',
    () async {
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(channel, (call) async {
            expect(call.method, 'extract');
            expect(call.arguments, {
              'requestId': id,
              'snapshotPath': '/private/staging/input.pdf',
              'maxInputBytes': limits.maxInputBytes,
              'maxPages': limits.maxPages,
              'maxTextBytes': limits.maxTextBytes,
              'maxDecompressedBytes': limits.maxDecompressedBytes,
            });
            return {
              'kind': 'extracted',
              'requestId': id,
              'complete': true,
              'pages': [
                {'physicalPage': 1, 'status': 'text', 'text': 'First'},
                {
                  'physicalPage': 2,
                  'status': 'no_extractable_text',
                  'text': '',
                },
              ],
            };
          });
      await expectLater(
        const PdfWorkerHost().extract(
          PdfWorkerRequest(
            requestId: id,
            snapshotPath: '/private/staging/input.pdf',
            limits: limits,
          ),
        ),
        throwsStateError, // Real generated Rust validation needs the native library.
      );
    },
  );

  test(
    'host rejects malformed typed result before returning success',
    () async {
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(
            channel,
            (_) async => {
              'kind': 'extracted',
              'requestId': id,
              'complete': true,
              'pages': [
                {'physicalPage': 2, 'status': 'text', 'text': 'invalid'},
              ],
            },
          );
      final reply = await const PdfWorkerHost().extract(
        PdfWorkerRequest(
          requestId: id,
          snapshotPath: '/private/staging/input.pdf',
          limits: limits,
        ),
      );
      expect(reply, isA<PdfWorkerReply_Failed>());
    },
  );

  test(
    'complete/status consistency rejects failed page reported complete',
    () async {
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(
            channel,
            (_) async => {
              'kind': 'extracted',
              'requestId': id,
              'complete': true,
              'pages': [
                {'physicalPage': 1, 'status': 'failed', 'text': ''},
              ],
            },
          );
      final reply = await const PdfWorkerHost().extract(
        PdfWorkerRequest(
          requestId: id,
          snapshotPath: '/private/staging/input.pdf',
          limits: limits,
        ),
      );
      expect(reply, const PdfWorkerReply.failed(PdfFailure.protocol));
    },
  );

  test(
    'success response maps to typed result without a native-library placeholder',
    () async {
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(
            channel,
            (_) async => {
              'kind': 'extracted',
              'requestId': id,
              'complete': true,
              'pages': [
                {'physicalPage': 1, 'status': 'text', 'text': 'ok'},
              ],
            },
          );
      final reply =
          await PdfWorkerHost(
            validate: (result, requestId, sentLimits) {
              expect(requestId, id);
              expect(sentLimits, limits);
              expect(result.extraction.pages.single.text, 'ok');
              return PdfWorkerReply.extracted(result);
            },
          ).extract(
            PdfWorkerRequest(
              requestId: id,
              snapshotPath: '/private/staging/input.pdf',
              limits: limits,
            ),
          );
      expect(reply, isA<PdfWorkerReply_Extracted>());
    },
  );

  test('one-over text limit rejects before native validator', () async {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          channel,
          (_) async => {
            'kind': 'extracted',
            'requestId': id,
            'complete': true,
            'pages': [
              {'physicalPage': 1, 'status': 'text', 'text': 'éé'},
            ],
          },
        );
    final reply =
        await PdfWorkerHost(
          validate: (_, _, _) =>
              throw StateError('must not validate oversized payload'),
        ).extract(
          PdfWorkerRequest(
            requestId: id,
            snapshotPath: '/private/staging/input.pdf',
            limits: const PdfLimits(
              maxInputBytes: 1,
              maxPages: 1,
              maxTextBytes: 3,
              maxDecompressedBytes: 1,
            ),
          ),
        );
    expect(reply, const PdfWorkerReply.failed(PdfFailure.limit));
  });

  test('cancel maps sanitized platform reply', () async {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (call) async {
          expect(call.method, 'cancel');
          expect(call.arguments, {'requestId': id});
          return {'kind': 'ok'};
        });
    expect(
      await const PdfWorkerHost().cancel(id),
      const PlatformCancelReply.ok(),
    );
  });
}
