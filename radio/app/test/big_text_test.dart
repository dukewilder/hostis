// The main screens at twice the text size, on a phone and on a computer: nothing may overflow.
// The node is answered by a stub (Native.stub), so no library is loaded; the fonts are the
// app's own, so the widths are the real ones.
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:pyrate/main.dart';
import 'package:pyrate/native.dart';
import 'package:pyrate/shell.dart';
import 'package:pyrate/station.dart';

const _id1 = 'f33eb5c67e9dd58ca8f85aa4c730f23081937ad9e99746829f7233dadff0803f';
const _id2 = '110243ae29e24f4f77bf4e5b0b9bb115ee4f2a4dfb76d6091fc913c54d97e44a';
const _id3 = '1243e4ce0ee6a4acbbf471d1e11b0325899d50880a80e5bc22fd65f999864069';

final _stations = <Map<String, dynamic>>[
  {
    'id': _id1,
    'name': 'The Public Domain With A Rather Long Name Indeed',
    'call_sign': 'FITABU·26BC',
    'about': 'Recordings published in 1925 or earlier, free in the United States since 1 January 2026, played round the clock.',
    'aboard': 1234,
    'live': false,
    'port': 'Bristol, England',
    'kinds': ['music', 'history', 'talk', 'readings', 'shanties'],
    'listed': true,
    'kept': true,
    'tracks': 412,
    'freq': 91.3,
  },
  {'id': _id2, 'name': 'Readings', 'call_sign': 'KOLEMA·77AA', 'about': 'LibriVox, all night.', 'aboard': 80, 'live': true, 'port': 'Austin, Texas', 'kinds': ['talk'], 'listed': true, 'freq': 97.1},
  {'id': _id3, 'name': 'The Numbers', 'call_sign': 'NUMBER·0001', 'about': '', 'aboard': 3, 'live': false, 'port': '', 'kinds': <String>[], 'listed': true, 'freq': 104.9},
];

// What the owner has already answered: the computer's start-up question is not asked.
// Whether this phone keeps a station of its own.
bool _own = false;

final _prefs = <String, dynamic>{'autostart': false};

dynamic _node(Map<String, dynamic> r) {
  switch (r['cmd']) {
    case 'status':
      return {
        'tuned': _id1,
        'state': 'playing',
        'volume': 1.0,
        'now': {'station': _id1, 'title': 'A Very Long Title For A Recording Of A Shanty Sung By A Crew', 'by': 'The Crew Of The Long Named Vessel', 'state': 'playing', 'position_ms': 30000, 'duration_ms': 180000},
        'neighbours': 4,
        'mine': _own
            ? {
                'id': _id2,
                'name': 'A Station Of My Own With A Long Name',
                'call_sign': 'KOLEMA·77AA',
                'tracks': 2,
                'carriers': 5,
                'link_only': false,
                'listable': true,
                'dial_at': DateTime.now().millisecondsSinceEpoch - 1000,
                'waiting': false,
                'direct': true,
              }
            : null,
      };
    case 'library':
      return _own
          ? [
              {'hash': 'aa', 'title': 'A Long Recording Title That Will Not Fit On One Line At Large Sizes', 'by': 'Somebody With A Long Name', 'duration_ms': 215000},
              {'hash': 'bb', 'title': 'Short', 'by': '', 'duration_ms': 61000},
            ]
          : <dynamic>[];
    case 'stations':
      return _stations;
    case 'pref':
      if (r.containsKey('value')) _prefs[r['key'] as String] = r['value'];
      return _prefs[r['key']];
    case 'tor':
      return {'url': ''};
    case 'door':
    case 'ballast_ok':
      return {};
    case 'page':
      return {
        'id': r['station'],
        'name': _stations[0]['name'],
        'about': _stations[0]['about'],
        'port': 'Bristol, England',
        'own': false,
        'listings': <dynamic>[],
        'notices': <dynamic>[],
        'tracks': 12,
        'next': <dynamic>[],
      };
    default:
      return <String, dynamic>{};
  }
}

Future<void> _loadFonts() async {
  const families = {
    'Cinzel': ['Cinzel-Regular.ttf', 'Cinzel-SemiBold.ttf', 'Cinzel-Black.ttf'],
    'Pica': ['IMFellDWPicaSC-Regular.ttf'],
    'Skull': ['NotoSansSymbols2-Skull.ttf'],
    'Fell': ['IMFellEnglish-Regular.ttf', 'IMFellEnglish-Italic.ttf'],
    'FellSC': ['IMFellEnglishSC-Regular.ttf'],
    'Mono': ['JetBrainsMono-Regular.ttf', 'JetBrainsMono-Bold.ttf'],
  };
  for (final e in families.entries) {
    final l = FontLoader(e.key);
    for (final f in e.value) {
      l.addFont(Future.value(ByteData.sublistView(File('assets/fonts/$f').readAsBytesSync())));
    }
    await l.load();
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final overflows = <String>[];
  late void Function(FlutterErrorDetails)? original;

  setUpAll(_loadFonts);

  // The test binding installs its own handler as each test starts, so ours goes in from inside.
  void Function() listen() {
    overflows.clear();
    Native.stub = _node;
    original = FlutterError.onError;
    FlutterError.onError = (d) {
      final text = d.exceptionAsString();
      if (text.contains('overflowed')) {
        // What overflowed, and the widget the framework names as the cause (file and line).
        final where = RegExp(r'(\w+:file:///\S+)').allMatches(d.toString()).map((m) => m.group(1)!.replaceFirst(RegExp(r'file:///.*/lib/'), '')).join(' ');
        final cause = '$where ${(d.informationCollector?.call() ?? const []).map((n) => n.toStringDeep()).join(' ').replaceAll(RegExp(r'\s+'), ' ')}';
        overflows.add('${text.split('\n').first} ${text.contains('Axis.') ? text.substring(text.indexOf('Axis.'), text.indexOf('Axis.') + 15) : ''} $cause');
      } else if (text.contains('ink splashes may be invisible')) {
        // A debug note about a folding row inside a coloured box: the app draws no splash, so
        // nothing is lost. It is not a layout fault and is left out of this test's business.
      } else {
        original?.call(d);
      }
    };
    return () {
      FlutterError.onError = original;
      Native.stub = null;
    };
  }

  for (final (label, size) in [('phone', const Size(360, 780)), ('computer', const Size(1280, 800))]) {
    for (final screen in ['Radio', 'Stations', 'Station page', 'Broadcast', 'Broadcast with a station of its own', 'Settings']) {
      testWidgets('$screen on a $label at 200 percent text', (tester) async {
        _own = screen.contains('own');
        final restore = listen();
        addTearDown(restore);
        tester.view.physicalSize = size;
        tester.view.devicePixelRatio = 1.0;
        tester.platformDispatcher.textScaleFactorTestValue = 2.0;
        addTearDown(tester.view.reset);
        addTearDown(tester.platformDispatcher.clearAllTestValues);

        await tester.pumpWidget(const PyrateApp());
        await tester.pump(const Duration(seconds: 1));
        // The text scale reaching the screens is the one asked for.
        final scaler = MediaQuery.of(tester.element(find.byType(Scaffold).first)).textScaler;
        expect(scaler.scale(10), 20);

        if (screen == 'Stations') await tester.tap(find.text('STATIONS'));
        if (screen.startsWith('Broadcast')) await tester.tap(find.text('BROADCAST'));
        if (screen == 'Settings') await tester.tap(find.text('SETTINGS'));
        if (screen == 'Station page') {
          final radio = (tester.state(find.byType(PyrateApp)) as dynamic).radio as Tuner;
          navKey.currentState!.push(MaterialPageRoute(builder: (_) => StationPage(station: _stations[0], radio: radio)));
        }
        await tester.pump(const Duration(seconds: 1));
        await tester.pump(const Duration(seconds: 1));
        // Long screens build the rest of their rows as they are scrolled to.
        for (var i = 0; i < 12; i++) {
          final lists = find.byType(Scrollable).hitTestable();
          if (lists.evaluate().isEmpty) break;
          await tester.drag(lists.first, const Offset(0, -500));
          await tester.pump(const Duration(milliseconds: 300));
        }
        final found = overflows.toSet().join('\n');

        // Let the tree go, and the timers the node's poll and the page left behind.
        await tester.pumpWidget(const SizedBox());
        await tester.pump(const Duration(minutes: 2));
        restore();
        expect(tester.takeException(), isNull);
        expect(found, isEmpty, reason: found);
      });
    }
  }
}
