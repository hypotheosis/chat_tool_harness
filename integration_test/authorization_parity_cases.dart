// Mirror of test/fixtures/authorization_parity.json, which the device cannot
// read; test/authorization_parity_cases_test.dart keeps the two identical and
// Rust's net_authorization_value_rule reads the JSON directly.
typedef ParityCase = ({String value, String? scheme, bool sendable});

const List<ParityCase> authorizationParityCases = [
  (value: 'Bearer x', scheme: 'Bearer', sendable: true),
  (value: 'Bearer ', scheme: null, sendable: false),
  (value: 'bare', scheme: null, sendable: true),
  (value: 'Bearer  x', scheme: 'Bearer', sendable: false),
  (value: 'Bearer\tx', scheme: null, sendable: false),
  (value: ' x', scheme: null, sendable: false),
  (value: '1abc def', scheme: null, sendable: true),
  (value: 'Token: x', scheme: null, sendable: true),
  (value: 'Béarer x', scheme: null, sendable: false),
  (value: 'Bearer x\n', scheme: 'Bearer', sendable: false),
  (value: 'Bearer x ', scheme: 'Bearer', sendable: false),
];
