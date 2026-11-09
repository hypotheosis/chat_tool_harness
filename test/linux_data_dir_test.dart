import 'package:chat_tool_harness/backend/models.dart';
import 'package:chat_tool_harness/backend/platform_services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const suffix = 'com.example.chat_tool_harness/agent';
  test('XDG_DATA_HOME wins when it is absolute', () {
    expect(
      linuxDataDir({'XDG_DATA_HOME': '/data', 'HOME': '/home/u'}),
      '/data/$suffix',
    );
    expect(linuxDataDir({'XDG_DATA_HOME': '/data/'}), '/data/$suffix');
  });
  test('an unset, empty or relative XDG_DATA_HOME falls back to HOME', () {
    for (final xdg in [null, '', 'relative/dir']) {
      expect(
        linuxDataDir({'XDG_DATA_HOME': ?xdg, 'HOME': '/home/u'}),
        '/home/u/.local/share/$suffix',
        reason: '$xdg',
      );
    }
  });
  test('no absolute base is a configuration failure', () {
    for (final env in [
      <String, String>{},
      {'HOME': ''},
      {'HOME': 'relative', 'XDG_DATA_HOME': 'also-relative'},
    ]) {
      expect(
        () => linuxDataDir(env),
        throwsA(
          isA<BackendFailure>().having(
            (e) => e.code,
            'code',
            FailureCode.configuration,
          ),
        ),
        reason: '$env',
      );
    }
  });
}
