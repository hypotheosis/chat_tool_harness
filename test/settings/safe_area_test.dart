import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:chat_tool_harness/features/profiles/profile_screen.dart';
import 'package:chat_tool_harness/features/profiles/tool_rules_screen.dart';
import 'package:chat_tool_harness/features/settings/endpoint_screen.dart';
import 'package:chat_tool_harness/features/settings/server_screen.dart';
import '../support/ui_fixture.dart';

/// 360 x [height] logical pixels with a 48 px navigation bar at the bottom.
const navigationBar = 48.0;

void main() {
  final cases = <String, (Widget Function(UiBackend), String, double)>{
    'server': (
      (b) => ServerScreen(backend: b, server: b.current.servers.single),
      'Test connection and discover tools',
      400,
    ),
    'endpoint': (
      (b) => EndpointScreen(backend: b, endpoint: endpoint),
      'Discover models',
      400,
    ),
    'profile': (
      (b) => ProfileScreen(backend: b, profile: profile),
      'Save profile',
      400,
    ),
    'tool rules': (
      (b) => ToolRulesScreen(backend: b),
      'No observed tools. Discover tools from a linked server or start an Agent conversation first.',
      240,
    ),
  };
  for (final MapEntry(key: name, value: (screen, last, height))
      in cases.entries) {
    testWidgets('last $name control is above the bottom system inset', (
      t,
    ) async {
      t.view.physicalSize = Size(1080, height * 3);
      t.view.devicePixelRatio = 3;
      t.view.padding = const FakeViewPadding(bottom: navigationBar * 3);
      t.view.viewPadding = const FakeViewPadding(bottom: navigationBar * 3);
      addTearDown(t.view.reset);
      await t.pumpWidget(MaterialApp(home: screen(UiBackend())));
      await t.pumpAndSettle();
      final list = find.byType(Scrollable).first;
      // The content must overflow so the list end, not its start, is checked.
      expect(
        t.state<ScrollableState>(list).position.maxScrollExtent,
        greaterThan(0),
      );
      final position = t.state<ScrollableState>(list).position;
      // Lazy layout can extend the list as it scrolls; repeat until settled.
      while (position.pixels < position.maxScrollExtent) {
        position.jumpTo(position.maxScrollExtent);
        await t.pumpAndSettle();
      }
      // The nearest Wrap holds the endpoint's diagnostic buttons, which may
      // wrap onto two lines; elsewhere the text itself is the last control.
      final wrap = find.ancestor(
        of: find.text(last),
        matching: find.byType(Wrap),
      );
      final target = name == 'endpoint' ? wrap.first : find.text(last);
      final bottom = t.getBottomLeft(target).dy;
      expect(bottom, lessThanOrEqualTo(height - navigationBar));
    });
  }
}
