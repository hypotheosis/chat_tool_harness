import 'dart:async';
import 'dart:convert';

import 'package:flutter/services.dart';

import 'src/rust/api/pdf.dart';
import 'src/rust/error.dart';
import 'src/rust/model.dart';

/// Main-process-only adapter. Kotlin owns input path validation and all FDs.
class PdfWorkerHost {
  const PdfWorkerHost({
    FutureOr<PdfWorkerReply> Function(PdfWorkerResult, String, PdfLimits)?
    validate,
  }) : _validate = validate ?? _validateNative;

  final FutureOr<PdfWorkerReply> Function(PdfWorkerResult, String, PdfLimits)
  _validate;

  static Future<PdfWorkerReply> _validateNative(
    PdfWorkerResult result,
    String requestId,
    PdfLimits limits,
  ) => validatePdfWorkerResult(
    result: result,
    requestId: requestId,
    limits: limits,
  );

  static const _channel = MethodChannel('chat_tool_harness/pdf_worker');

  Future<PdfWorkerReply> extract(PdfWorkerRequest request) async {
    try {
      final response = await _channel
          .invokeMapMethod<String, dynamic>('extract', {
            'requestId': request.requestId,
            'snapshotPath': request.snapshotPath,
            'maxInputBytes': request.limits.maxInputBytes,
            'maxPages': request.limits.maxPages,
            'maxTextBytes': request.limits.maxTextBytes,
            'maxDecompressedBytes': request.limits.maxDecompressedBytes,
          });
      if (response == null) {
        return const PdfWorkerReply.failed(PdfFailure.protocol);
      }
      if (response['kind'] == 'failed') {
        return PdfWorkerReply.failed(_failure(response['code']));
      }
      if (response['kind'] != 'extracted' ||
          response['requestId'] != request.requestId) {
        return const PdfWorkerReply.failed(PdfFailure.protocol);
      }
      final rawPages = response['pages'];
      if (rawPages is! List ||
          rawPages.isEmpty ||
          rawPages.length > request.limits.maxPages) {
        return const PdfWorkerReply.failed(PdfFailure.limit);
      }
      var textBytes = 0;
      final pages = <PageText>[];
      for (final raw in rawPages) {
        if (raw is! Map ||
            raw['physicalPage'] != pages.length + 1 ||
            raw['text'] is! String) {
          return const PdfWorkerReply.failed(PdfFailure.protocol);
        }
        final text = raw['text'] as String;
        textBytes += utf8.encode(text).length;
        if (textBytes > request.limits.maxTextBytes) {
          return const PdfWorkerReply.failed(PdfFailure.limit);
        }
        final status = switch (raw['status']) {
          'text' => PageStatus.text,
          'no_extractable_text' => PageStatus.noExtractableText,
          'failed' => PageStatus.failed,
          _ => null,
        };
        if (status == null) {
          return const PdfWorkerReply.failed(PdfFailure.protocol);
        }
        pages.add(
          PageText(
            physicalPage: raw['physicalPage'] as int,
            text: text,
            status: status,
          ),
        );
      }
      if (response['complete'] is! bool) {
        return const PdfWorkerReply.failed(PdfFailure.protocol);
      }
      if ((response['complete'] as bool) !=
              pages.every((page) => page.status != PageStatus.failed) ||
          pages.any(
            (page) => (page.status == PageStatus.text) != page.text.isNotEmpty,
          )) {
        return const PdfWorkerReply.failed(PdfFailure.protocol);
      }
      final result = PdfWorkerResult(
        requestId: request.requestId,
        extraction: PdfExtraction(
          pages: pages,
          complete: response['complete'] as bool,
        ),
      );
      return await _validate(result, request.requestId, request.limits);
    } on PlatformException catch (e) {
      return PdfWorkerReply.failed(_failure(e.code));
    } on Exception {
      return const PdfWorkerReply.failed(PdfFailure.protocol);
    }
  }

  Future<PlatformCancelReply> cancel(String requestId) async {
    try {
      final response = await _channel.invokeMapMethod<String, dynamic>(
        'cancel',
        {'requestId': requestId},
      );
      return response?['kind'] == 'ok'
          ? const PlatformCancelReply.ok()
          : const PlatformCancelReply.failed('unavailable');
    } on PlatformException {
      return const PlatformCancelReply.failed('unavailable');
    }
  }
}

class PdfWorkerRequest {
  final String requestId;
  final String snapshotPath;
  final PdfLimits limits;
  const PdfWorkerRequest({
    required this.requestId,
    required this.snapshotPath,
    required this.limits,
  });
}

sealed class PlatformCancelReply {
  const PlatformCancelReply();
  const factory PlatformCancelReply.ok() = PlatformCancelOk;
  const factory PlatformCancelReply.failed(String code) = PlatformCancelFailed;
}

class PlatformCancelOk extends PlatformCancelReply {
  const PlatformCancelOk();
  @override
  bool operator ==(Object other) => other is PlatformCancelOk;
  @override
  int get hashCode => 0;
}

class PlatformCancelFailed extends PlatformCancelReply {
  final String code;
  const PlatformCancelFailed(this.code);
  @override
  bool operator ==(Object other) =>
      other is PlatformCancelFailed && other.code == code;
  @override
  int get hashCode => code.hashCode;
}

PdfFailure _failure(Object? code) => switch (code) {
  'busy' => PdfFailure.busy,
  'limit' => PdfFailure.limit,
  'invalid_input' => PdfFailure.invalidInput,
  'unsupported_encrypted' => PdfFailure.unsupportedEncrypted,
  'timeout' => PdfFailure.timeout,
  'worker_died' => PdfFailure.workerDied,
  'cancelled' => PdfFailure.cancelled,
  'unavailable' => PdfFailure.unavailable,
  _ => PdfFailure.protocol,
};
