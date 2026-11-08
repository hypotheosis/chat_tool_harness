import 'models.dart';

/// Check before fixed-width encoding can discard the original external value.
BigInt bridgeUnsigned(BigInt value, int bits) {
  if (value.isNegative || value >= (BigInt.one << bits)) {
    throw const BackendFailure(FailureCode.invalidInput);
  }
  return value;
}
