import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/main.dart' as app;
import 'package:chat_tool_harness/backend/platform_services.dart';
import 'package:chat_tool_harness/src/rust/api/simple.dart';
import 'package:chat_tool_harness/src/rust/frb_generated.dart';
import 'package:integration_test/integration_test.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets(
    'app startup initializes native backend and opens conversations',
    (tester) async {
      await app.main();
      await tester.pumpAndSettle();
      expect(RustLib.instance.initialized, isTrue);
      expect(find.text('Conversations'), findsOneWidget);
      expect(
        (await (await EngineBackend.owner.backend).snapshot()).sequence,
        greaterThanOrEqualTo(0),
      );
      expect(greet(name: 'Tom'), 'Hello, Tom!');
      await EngineBackend.close();
    },
  );
}
