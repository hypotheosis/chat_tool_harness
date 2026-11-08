import 'dart:convert';
import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import '../integration_test/authorization_parity_cases.dart';

void main() {
  test('device parity cases match the shared fixture Rust tests', () {
    final fixture =
        jsonDecode(
              File(
                'test/fixtures/authorization_parity.json',
              ).readAsStringSync(),
            )
            as List<Object?>;
    expect(
      authorizationParityCases
          .map(
            (c) => {
              'value': c.value,
              'scheme': c.scheme,
              'sendable': c.sendable,
            },
          )
          .toList(),
      fixture,
    );
  });
}
