import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/backend/bridge_numbers.dart';
import 'package:chat_tool_harness/backend/models.dart';

void main() {
  for (final bits in [32, 64]) {
    test('unsigned_${bits}_preserves_boundaries_and_rejects_narrowing', () {
      final max = (BigInt.one << bits) - BigInt.one;
      for (final value in [BigInt.zero, BigInt.one, max]) {
        expect(bridgeUnsigned(value, bits), value);
      }
      for (final value in [-BigInt.one, max + BigInt.one, max + BigInt.two]) {
        expect(
          () => bridgeUnsigned(value, bits),
          throwsA(
            isA<BackendFailure>().having(
              (e) => e.code,
              'code',
              FailureCode.invalidInput,
            ),
          ),
        );
      }
    });
  }
}
