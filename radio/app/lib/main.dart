// Pyrate Radio. No rights reserved.

import 'dart:async';
import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';

import 'art.dart';
import 'more.dart';
import 'native.dart';
import 'station.dart';

// The site's ink.
const black = Color(0xFF030303);
const panel = Color(0xFF0F0F0F);
const bone = Color(0xFFEDE4CE);
const boneDim = Color(0xD1EDE4CE);
const boneFaint = Color(0x8CEDE4CE);
const boneGhost = Color(0x38EDE4CE);
const blood = Color(0xFFC52828);
const bloodBright = Color(0xFFED5555);
const brass = Color(0xFFC9A94D);

TextStyle carved(double size, {Color color = bone, double spacing = 0.3}) =>
    TextStyle(fontFamily: 'Cinzel', fontSize: size, color: color, letterSpacing: size * spacing, height: 1.2);
TextStyle serif(double size, {Color color = bone, bool italic = false}) =>
    TextStyle(fontFamily: 'Fell', fontSize: size, color: color, fontStyle: italic ? FontStyle.italic : FontStyle.normal, height: 1.35);
TextStyle mono(double size, {Color color = boneFaint, FontWeight weight = FontWeight.w400}) =>
    TextStyle(fontFamily: 'Mono', fontSize: size, color: color, letterSpacing: size * 0.05, fontWeight: weight);

const _service = MethodChannel('pyrate/service');

/// This build's version code. A release is offered only when its code is higher.
const kBuildCode = 12;

/// The stations a first open lands on when no station has yet earned it: HOSTIS's own.
List<String> harbour = [];

void main() async {
  WidgetsFlutterBinding.ensureInitialized();
  SystemChrome.setSystemUIOverlayStyle(const SystemUiOverlayStyle(
    statusBarColor: black,
    systemNavigationBarColor: black,
    statusBarIconBrightness: Brightness.light,
  ));
  final dir = await getApplicationSupportDirectory();
  final boot = (await rootBundle.loadString('assets/bootstrap.txt')).trim();
  // HOSTIS's own stations, where a first open lands, and the keys that sign releases.
  harbour = (await rootBundle.loadString('assets/harbour.txt'))
      .split('\n')
      .map((l) => l.trim().split(RegExp(r'\s+')).first)
      .where((l) => RegExp(r'^[0-9a-f]{64}$').hasMatch(l))
      .toList();
  Native.setReleaseKeys(await rootBundle.loadString('assets/release_keys.txt'));
  final started = Native.start('${dir.path}${Platform.pathSeparator}node', boot);
  print('pyrate: node start $started');
  runApp(const PyrateApp());
}

/// Everything the screens show, polled from the node.
class Tuner extends ChangeNotifier {
  List<Map<String, dynamic>> stations = [];
  Map<String, dynamic> status = {};
  String? tuned;
  bool playing = false;
  bool scanning = false;
  bool firstTuneDone = false;
  Timer? _poll;
  Timer? _scan;
  int _ticks = 0;
  final pages = PageController();

  /// What has played this session, newest last. Memory only; gone when the app closes.
  final List<Map<String, dynamic>> heard = [];
  DateTime? sleepAt;
  Timer? _sleep;
  int _tunedAt = 0;

  int carrying = 0;
  Timer? _ballast;
  Timer? _update;
  Map<String, dynamic>? update;

  Tuner() {
    _poll = Timer.periodic(const Duration(milliseconds: 700), (_) => _refresh());
    // Ballast fills only on Wi-Fi while charging, so the fleet never costs the owner.
    _ballast = Timer.periodic(const Duration(seconds: 60), (_) => _checkBallast());
    _checkBallast();
    // New versions arrive over the network itself, signed by two of three release keys.
    Future.delayed(const Duration(seconds: 60), _checkUpdate);
    _update = Timer.periodic(const Duration(minutes: 30), (_) => _checkUpdate());
    _refresh();
  }

  Map<String, dynamic>? get mine => status['mine'] as Map<String, dynamic>?;
  Map<String, dynamic>? get now => status['now'] as Map<String, dynamic>?;
  List get jobs => (status['jobs'] as List?) ?? [];

  Map<String, dynamic>? station(String? id) {
    for (final s in stations) {
      if (s['id'] == id) return s;
    }
    return null;
  }

  Future<void> _checkUpdate() async {
    // Releases come down only on Wi-Fi or another unmetered network.
    final c = await _service.invokeMethod<Map>('conditions').catchError((_) => <dynamic, dynamic>{}) ?? {};
    if (c['unmetered'] != true) return;
    final r = await Native.callAway({'cmd': 'update', 'code': kBuildCode});
    if (r is Map && r['available'] == true) {
      update = r.cast<String, dynamic>();
      notifyListeners();
    }
  }

  Future<void> _checkBallast() async {
    final c = await _service.invokeMethod<Map>('conditions').catchError((_) => <dynamic, dynamic>{}) ?? {};
    final unmetered = c['unmetered'] == true;
    final r = Native.call({'cmd': 'ballast_ok', 'value': unmetered && c['charging'] == true, 'unmetered': unmetered});
    if (r is Map && r['carrying'] is int) carrying = r['carrying'];
  }

  /// Fades out and stops after [d]; null cancels.
  void sleepIn(Duration? d) {
    _sleep?.cancel();
    sleepAt = d == null ? null : DateTime.now().add(d);
    if (d != null) {
      _sleep = Timer(d, () async {
        for (var v = 0.9; v > 0.05; v -= 0.1) {
          Native.call({'cmd': 'volume', 'value': v});
          await Future.delayed(const Duration(seconds: 3));
        }
        stop();
        Native.call({'cmd': 'volume', 'value': 1.0});
        sleepAt = null;
        notifyListeners();
      });
    }
    notifyListeners();
  }

  void _refresh() {
    _ticks++;
    final st = Native.call({'cmd': 'status'});
    if (st is Map<String, dynamic> && st['error'] == null) status = st;
    // The lock screen, headphones or the car may have changed station or stopped the radio.
    final nodeTuned = status['tuned'] as String?;
    if (DateTime.now().millisecondsSinceEpoch - _tunedAt > 3000) {
      if (nodeTuned == null && playing) {
        playing = false;
      } else if (nodeTuned != null && nodeTuned != tuned) {
        tuned = nodeTuned;
        playing = true;
        final i = stations.indexWhere((s) => s['id'] == nodeTuned);
        if (i >= 0 && pages.hasClients && pages.page?.round() != i) pages.jumpToPage(i);
      } else if (nodeTuned != null && !playing) {
        playing = true;
      }
    }
    final n = now;
    if (n != null && (n['title'] ?? '').toString().isNotEmpty && n['state'] == 'playing') {
      final last = heard.isEmpty ? null : heard.last;
      if (last == null || last['title'] != n['title'] || last['id'] != n['station']) {
        heard.add({'at': DateTime.now().millisecondsSinceEpoch, 'title': n['title'], 'by': n['by'], 'id': n['station'], 'station': station(n['station'] as String?)?['name'] ?? ''});
        if (heard.length > 200) heard.removeAt(0);
      }
    }
    if (_ticks % 4 == 2) {
      // A station link or a bell's tap, waiting to be followed.
      _service.invokeMethod<String>('take_link').then((id) {
        if (id != null) {
          firstTuneDone = true;
          pendingLink = id;
        }
      }).catchError((_) {});
    }
    if (pendingLink != null && station(pendingLink) != null) {
      final id = pendingLink!;
      pendingLink = null;
      tune(id);
    }
    if (_ticks % 3 == 1 || stations.isEmpty) {
      final list = Native.call({'cmd': 'stations'});
      if (list is List) {
        stations = list.cast<Map<String, dynamic>>();
        for (final s in stations) {
          if (harbour.contains(s['id'])) harbourNames.add((s['name'] ?? '').toString().trim().toLowerCase());
        }
        if (tuned != null && station(tuned) == null && stations.isNotEmpty) {
          // Keep the tuned station visible even if its card lapses for a moment.
        }
      }
    }
    // First open plays sound: the station with the most aboard.
    // A first open lands on the busiest station that has earned it: a week on the air and
    // three independent carriers. A bought count alone cannot win a stranger's first minute.
    if (!firstTuneDone && stations.isNotEmpty) {
      final now = DateTime.now().millisecondsSinceEpoch;
      final earned = stations.where((s) =>
          s['kept'] == true ||
          ((s['carriers'] as num? ?? 0) >= 3 && now - ((s['since'] as num?)?.toInt() ?? now) > 7 * 86400000)).toList();
      final home = stations.where((s) => harbour.contains(s['id'])).toList();
      final pick = earned.isNotEmpty ? earned.first : (home.isNotEmpty ? home.first : null);
      if (pick != null || _ticks > 20) {
        firstTuneDone = true;
        tune((pick ?? stations.first)['id'] as String);
      }
    }
    notifyListeners();
  }

  String? pendingLink;

  void tune(String id) {
    print('pyrate: tune $id');
    tuned = id;
    playing = true;
    _tunedAt = DateTime.now().millisecondsSinceEpoch;
    _service.invokeMethod('remember', {'station': id}).catchError((_) {});
    Native.call({'cmd': 'tune', 'station': id});
    final name = station(id)?['name'] ?? 'Pyrate Radio';
    _service.invokeMethod('start', {'title': name}).catchError((_) {});
    final i = stations.indexWhere((s) => s['id'] == id);
    if (i >= 0 && pages.hasClients && pages.page?.round() != i) {
      pages.jumpToPage(i);
    }
    notifyListeners();
  }

  void stop() {
    playing = false;
    _tunedAt = DateTime.now().millisecondsSinceEpoch;
    stopScan();
    Native.call({'cmd': 'stop'});
    _service.invokeMethod('stop').catchError((_) {});
    notifyListeners();
  }

  void toggle() {
    if (playing) {
      stop();
    } else if (tuned != null) {
      tune(tuned!);
    } else if (stations.isNotEmpty) {
      tune(stations.first['id'] as String);
    }
  }

  void startScan() {
    if (stations.length < 2) return;
    scanning = true;
    _scan?.cancel();
    _scan = Timer.periodic(const Duration(seconds: 4), (_) {
      if (!pages.hasClients) return;
      final next = ((pages.page ?? 0).round() + 1) % stations.length;
      pages.animateToPage(next, duration: const Duration(milliseconds: 400), curve: Curves.easeOut);
    });
    notifyListeners();
  }

  void stopScan() {
    scanning = false;
    _scan?.cancel();
    notifyListeners();
  }

  @override
  void dispose() {
    _poll?.cancel();
    _scan?.cancel();
    _ballast?.cancel();
    _update?.cancel();
    super.dispose();
  }
}

class PyrateApp extends StatefulWidget {
  const PyrateApp({super.key});
  @override
  State<PyrateApp> createState() => _PyrateAppState();
}

class _PyrateAppState extends State<PyrateApp> {
  final radio = Tuner();
  int tab = 0;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Pyrate Radio',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        brightness: Brightness.dark,
        scaffoldBackgroundColor: black,
        colorScheme: const ColorScheme.dark(primary: bone, surface: black, onSurface: bone, secondary: bloodBright),
        textSelectionTheme: const TextSelectionThemeData(cursorColor: bone, selectionColor: boneGhost),
        splashFactory: NoSplash.splashFactory,
        highlightColor: Colors.transparent,
      ),
      home: ListenableBuilder(
        listenable: radio,
        builder: (context, _) => Scaffold(
          body: SafeArea(
            child: IndexedStack(index: tab, children: [
              Dial(radio: radio),
              LogPage(radio: radio, onTune: () => setState(() => tab = 0)),
              PinGate(child: ColoursPage(radio: radio)),
            ]),
          ),
          bottomNavigationBar: SafeArea(
            top: false,
            child: Container(
              decoration: const BoxDecoration(border: Border(top: BorderSide(color: boneGhost, width: 0.5))),
              height: 58,
              child: Row(children: [
                for (final (i, label) in ['THE DIAL', 'THE LOG', 'COLOURS'].indexed)
                  Expanded(
                    child: InkWell(
                      onTap: () => setState(() => tab = i),
                      child: Center(
                        child: Text(label, style: carved(11, color: tab == i ? bone : boneFaint, spacing: 0.25)),
                      ),
                    ),
                  ),
              ]),
            ),
          ),
        ),
      ),
    );
  }
}

/// A station's flag, drawn from its key: binary digits, mirrored, bone on black.
/// Two stations can never share one, and nobody can forge one.
class Flag extends StatelessWidget {
  final String id;
  final double width;
  final bool live;
  const Flag({super.key, required this.id, required this.width, this.live = false});

  @override
  Widget build(BuildContext context) => CustomPaint(
        size: Size(width, width * 2 / 3),
        painter: _FlagPainter(id, live),
      );
}

class _FlagPainter extends CustomPainter {
  final String id;
  final bool live;
  _FlagPainter(this.id, this.live);

  @override
  void paint(Canvas canvas, Size size) {
    final rect = Offset.zero & size;
    canvas.drawRect(rect, Paint()..color = black);
    canvas.drawRect(rect.deflate(0.5), Paint()
      ..color = live ? blood : boneGhost
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1);
    const cols = 9, rows = 6;
    final bits = <bool>[];
    for (var i = 0; i < id.length && bits.length < 64; i++) {
      final v = int.tryParse(id[i], radix: 16) ?? 0;
      for (var b = 3; b >= 0; b--) {
        bits.add((v >> b) & 1 == 1);
      }
    }
    final cw = size.width / (cols + 2), ch = size.height / (rows + 2);
    final fontSize = ch * 0.9;
    for (var r = 0; r < rows; r++) {
      for (var c = 0; c < cols; c++) {
        final half = c < (cols + 1) ~/ 2 ? c : cols - 1 - c;
        final on = bits.isEmpty ? false : bits[(r * ((cols + 1) ~/ 2) + half) % bits.length];
        final tp = TextPainter(
          text: TextSpan(text: on ? '1' : '0', style: TextStyle(fontFamily: 'Mono', fontSize: fontSize, color: on ? bone : const Color(0x1FEDE4CE), fontWeight: on ? FontWeight.w700 : FontWeight.w400)),
          textDirection: TextDirection.ltr,
        )..layout();
        tp.paint(canvas, Offset(cw * (c + 1) + (cw - tp.width) / 2, ch * (r + 1) + (ch - tp.height) / 2));
      }
    }
  }

  @override
  bool shouldRepaint(covariant _FlagPainter old) => old.id != id || old.live != live;
}

String fmtTime(int ms) {
  final s = ms ~/ 1000;
  return '${s ~/ 60}:${(s % 60).toString().padLeft(2, '0')}';
}

class Dial extends StatelessWidget {
  final Tuner radio;
  const Dial({super.key, required this.radio});

  @override
  Widget build(BuildContext context) {
    final stations = radio.stations;
    final up = radio.update;
    return Column(children: [
      if (up != null)
        InkWell(
          onTap: () => _service.invokeMethod('install', {'path': up['path']}).catchError((_) {}),
          child: Container(
            width: double.infinity,
            padding: const EdgeInsets.symmetric(vertical: 10, horizontal: 20),
            decoration: const BoxDecoration(border: Border(bottom: BorderSide(color: brass, width: 0.5))),
            child: Text('A NEW VERSION IS ABOARD · ${up['version']} · TAP TO INSTALL', textAlign: TextAlign.center, style: carved(10, color: brass, spacing: 0.2)),
          ),
        ),
      const SizedBox(height: 18),
      Image.asset('assets/brand/wordmark.png', width: 210, filterQuality: FilterQuality.medium, color: boneDim, colorBlendMode: BlendMode.modulate),
      const SizedBox(height: 2),
      Text('UNDER HOSTIS COLOURS', style: TextStyle(fontFamily: 'FellSC', fontSize: 10, color: boneGhost, letterSpacing: 2.4)),
      const SizedBox(height: 4),
      Text(stations.isEmpty ? '' : '${stations.length} ${stations.length == 1 ? 'STATION' : 'STATIONS'} ON THE AIR',
          style: mono(10, color: boneGhost)),
      Expanded(
        child: stations.isEmpty
            ? Center(
                child: Column(mainAxisSize: MainAxisSize.min, children: [
                  Text('Searching the Log', style: serif(22, color: boneDim, italic: true)),
                  const SizedBox(height: 10),
                  const SizedBox(width: 120, child: LinearProgressIndicator(minHeight: 1, color: boneFaint, backgroundColor: boneGhost)),
                ]),
              )
            : GestureDetector(
                onLongPress: radio.startScan,
                onTap: radio.scanning ? radio.stopScan : null,
                child: PageView.builder(
                  controller: radio.pages,
                  itemCount: stations.length,
                  onPageChanged: (i) {
                    final id = stations[i]['id'] as String;
                    if (id != radio.tuned) radio.tune(id);
                  },
                  itemBuilder: (context, i) => _StationFace(radio: radio, s: stations[i]),
                ),
              ),
      ),
      Padding(
        padding: const EdgeInsets.only(bottom: 18, top: 6),
        child: radio.scanning
            ? Text('SCANNING · TAP TO HOLD', style: carved(11, color: brass, spacing: 0.25))
            : Row(mainAxisAlignment: MainAxisAlignment.center, children: [
                OutlinedButton(
                  onPressed: stations.isEmpty ? null : radio.toggle,
                  style: OutlinedButton.styleFrom(
                    side: const BorderSide(color: boneFaint, width: 0.8),
                    shape: const RoundedRectangleBorder(),
                    padding: const EdgeInsets.symmetric(horizontal: 34, vertical: 14),
                  ),
                  child: Text(radio.playing ? '■  STOP' : '▶  LISTEN', style: carved(12, spacing: 0.25)),
                ),
              ]),
      ),
      Text('swipe to change station · hold to scan', style: serif(12, color: boneGhost, italic: true)),
      const SizedBox(height: 10),
    ]);
  }
}

class _StationFace extends StatelessWidget {
  final Tuner radio;
  final Map<String, dynamic> s;
  const _StationFace({required this.radio, required this.s});

  @override
  Widget build(BuildContext context) {
    final id = s['id'] as String;
    final live = s['live'] == true;
    final isTuned = radio.tuned == id && radio.playing;
    final now = isTuned ? radio.now : null;
    final state = now?['state'] as String?;
    final title = (now?['title'] as String?) ?? '';
    final by = (now?['by'] as String?) ?? '';
    final pos = (now?['position_ms'] as num?)?.toInt() ?? 0;
    final dur = (now?['duration_ms'] as num?)?.toInt() ?? 0;
    final accent = accentOf(s['accent']) ?? bone;
    final pic = (isTuned && now?['art'] != null) ? now!['art'] as String : s['cover'] as String?;
    return Stack(children: [
      if (s['banner'] != null)
        Positioned(
          top: 0, left: 0, right: 0, height: 220,
          child: ShaderMask(
            blendMode: BlendMode.dstIn,
            shaderCallback: (r) => const LinearGradient(begin: Alignment.topCenter, end: Alignment.bottomCenter, colors: [Color(0x88FFFFFF), Color(0x00FFFFFF)]).createShader(r),
            child: StationArt(station: id, hash: s['banner'], width: double.infinity, height: 220, fallback: const SizedBox()),
          ),
        ),
      Padding(
      padding: const EdgeInsets.symmetric(horizontal: 28),
      child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [
        pic == null
            ? Flag(id: id, width: 200, live: live)
            : Container(
                decoration: BoxDecoration(border: Border.all(color: accent.withValues(alpha: 0.45), width: 0.6)),
                child: StationArt(station: id, hash: pic, width: 220, height: 220, fallback: Flag(id: id, width: 180, live: live)),
              ),
        const SizedBox(height: 26),
        if (live)
          Container(
            margin: const EdgeInsets.only(bottom: 12),
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 3),
            decoration: BoxDecoration(border: Border.all(color: bloodBright, width: 0.8)),
            child: Text('ON AIR', style: carved(11, color: bloodBright, spacing: 0.35)),
          ),
        GestureDetector(
          onTap: () => openPage(context, s, radio),
          child: Row(mainAxisSize: MainAxisSize.min, children: [
            Flexible(child: Text(s['name'] ?? '', textAlign: TextAlign.center, style: serif(34))),
            ...seals(s, size: 10),
          ]),
        ),
        const SizedBox(height: 6),
        Text(s['call_sign'] ?? '', style: mono(12, color: boneFaint)),
        const SizedBox(height: 8),
        Wrap(alignment: WrapAlignment.center, spacing: 2, children: [
          for (final (label, act) in <(String, VoidCallback)>[
            ('THE PAGE', () => openPage(context, s, radio)),
            ('WATCH BILL', () => pyrateSheet(context, WatchBill(station: s))),
            if (isTuned) ('TAPE IT', () => tapeIt(context)),
            ('PASS IT ON', () => shareStation(s)),
            if ((s['tip'] ?? '').toString().isNotEmpty) ('TIP', () => tipStation(context, s['tip'])),
          ])
            TextButton(
              onPressed: act,
              style: TextButton.styleFrom(padding: const EdgeInsets.symmetric(horizontal: 8), minimumSize: const Size(0, 30)),
              child: Text(label, style: carved(9.5, color: boneFaint, spacing: 0.25)),
            ),
          IconButton(
            tooltip: 'Lights out',
            visualDensity: VisualDensity.compact,
            icon: Icon(radio.sleepAt == null ? Icons.bedtime_outlined : Icons.bedtime, size: 16, color: radio.sleepAt == null ? boneGhost : brass),
            onPressed: () => sleepSheet(context, radio),
          ),
          IconButton(
            tooltip: 'Heard tonight',
            visualDensity: VisualDensity.compact,
            icon: const Icon(Icons.history, size: 16, color: boneGhost),
            onPressed: () => historySheet(context, radio),
          ),
        ]),
        const SizedBox(height: 14),
        if ((s['about'] ?? '').toString().isNotEmpty)
          Text(s['about'], textAlign: TextAlign.center, maxLines: 3, overflow: TextOverflow.ellipsis,
              style: serif(15, color: boneFaint, italic: true)),
        const SizedBox(height: 28),
        SizedBox(
          height: 92,
          child: !isTuned
              ? const SizedBox()
              : state == 'searching' && title.isEmpty
                  ? Text('Finding a carrier', style: serif(16, color: boneFaint, italic: true))
                  : state == 'dead air'
                      ? Text('Dead air', style: serif(16, color: boneFaint, italic: true))
                      : state == 'off air'
                      ? Text(offAirLabel((now?['back_at'] as num?)?.toInt()), textAlign: TextAlign.center, style: serif(16, color: boneFaint, italic: true))
                      : Column(children: [
                          if (state != 'live' && (now?['slot'] ?? '').toString().isNotEmpty)
                            Text((now!['slot']).toString().toUpperCase(), maxLines: 1, overflow: TextOverflow.ellipsis, style: carved(9.5, color: accent, spacing: 0.3)),
                          Text(state == 'live' ? 'Live' : title, textAlign: TextAlign.center, maxLines: 2,
                              overflow: TextOverflow.ellipsis, style: serif(19, color: boneDim, italic: true)),
                          if (by.isNotEmpty) Text(by, style: carved(11, color: boneFaint, spacing: 0.2)),
                          const SizedBox(height: 12),
                          if (dur > 0) ...[
                            SizedBox(
                              width: 220,
                              child: LinearProgressIndicator(value: (pos / dur).clamp(0.0, 1.0), minHeight: 1, color: accent, backgroundColor: boneGhost),
                            ),
                            const SizedBox(height: 6),
                            Text('${fmtTime(pos)} / ${fmtTime(dur)}', style: mono(10, color: boneGhost)),
                          ],
                        ]),
        ),
        Text('${s['aboard'] ?? 0} ABOARD', style: mono(12, color: boneFaint, weight: FontWeight.w700)),
      ]),
      ),
    ]);
  }
}

class ColoursPage extends StatefulWidget {
  final Tuner radio;
  const ColoursPage({super.key, required this.radio});
  @override
  State<ColoursPage> createState() => _ColoursPageState();
}

class _ColoursPageState extends State<ColoursPage> {
  final name = TextEditingController();
  final about = TextEditingController();
  final restoreWords = TextEditingController();
  bool raising = false;
  bool talk = false;
  String? error;
  List library = [];
  String? libraryFor;
  int libraryCount = -1;
  final shape = StartShape();
  Map? programme;
  String? programmeFor;

  InputDecoration field(String label) => InputDecoration(
        labelText: label,
        labelStyle: serif(15, color: boneFaint, italic: true),
        enabledBorder: const UnderlineInputBorder(borderSide: BorderSide(color: boneGhost)),
        focusedBorder: const UnderlineInputBorder(borderSide: BorderSide(color: bone)),
      );

  Widget button(String label, VoidCallback? onTap, {Color color = bone}) => OutlinedButton(
        onPressed: onTap,
        style: OutlinedButton.styleFrom(
          side: BorderSide(color: onTap == null ? boneGhost : color.withValues(alpha: 0.7), width: 0.8),
          shape: const RoundedRectangleBorder(),
          padding: const EdgeInsets.symmetric(horizontal: 22, vertical: 13),
        ),
        child: Text(label, style: carved(11, color: onTap == null ? boneGhost : color, spacing: 0.25)),
      );

  Future<void> raise() async {
    setState(() {
      raising = true;
      error = null;
    });
    final r = await Native.callAway({'cmd': 'raise', 'name': name.text, 'about': about.text});
    if (r is Map && r['id'] is String) await shape.apply(r['id']);
    setState(() {
      raising = false;
      if (r is Map && r['error'] != null) error = r['error'];
    });
    if (r is Map && r['words'] is String && mounted) {
      await showDialog(
        context: context,
        barrierDismissible: false,
        builder: (c) => AlertDialog(
          backgroundColor: panel,
          shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
          title: Text('Write these down', style: serif(24)),
          content: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text('These 24 words are the station. This phone keeps only a deputy key, which can broadcast but cannot strike. If the phone is lost or taken, the words take the station back and shut the deputy out. They are shown once and kept nowhere.',
                style: serif(15, color: boneFaint, italic: true)),
            const SizedBox(height: 14),
            Container(
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(color: black, border: Border.all(color: boneGhost, width: 0.5)),
              child: SelectableText(r['words'], style: mono(13, color: bone)),
            ),
          ]),
          actions: [TextButton(onPressed: () => Navigator.pop(c), child: Text('THEY ARE WRITTEN DOWN', style: carved(11)))],
        ),
      );
    }
  }

  String? artNote;

  Future<void> _setArt(String station, String kind, String path) async {
    setState(() => artNote = 'Redrawing the picture');
    final r = await Native.callAway({'cmd': 'art_set', 'station': station, 'kind': kind, 'path': path});
    setState(() => artNote = r is Map && r['error'] != null ? 'That picture would not take: ${r['error']}' : null);
  }

  Future<void> pickArt(String station, String kind) async {
    final picked = await FilePicker.platform.pickFiles(type: FileType.image);
    final path = picked?.files.single.path;
    if (path != null) await _setArt(station, kind, path);
  }

  Future<void> linkArt(String station, String kind) async {
    final link = TextEditingController();
    final url = await showDialog<String>(
      context: context,
      builder: (c) => AlertDialog(
        backgroundColor: panel,
        shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
        title: Text(kind == 'banner' ? 'BANNER FROM A LINK' : 'COVER FROM A LINK', style: carved(13)),
        content: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text('Your phone fetches the picture once and redraws it. The site you link sees this phone\'s address; listeners never touch it.',
              style: serif(13, color: boneFaint, italic: true)),
          TextField(controller: link, autofocus: true, style: mono(12, color: bone), decoration: field('https://')),
        ]),
        actions: [TextButton(onPressed: () => Navigator.pop(c, link.text.trim()), child: Text('FETCH', style: carved(11)))],
      ),
    );
    if (url == null || !url.startsWith('http')) return;
    setState(() => artNote = 'Fetching the picture');
    try {
      final req = await HttpClient().getUrl(Uri.parse(url));
      final res = await req.close();
      final bytes = <int>[];
      await for (final part in res) {
        bytes.addAll(part);
        if (bytes.length > 30000000) throw 'too large';
      }
      final f = File('${Directory.systemTemp.path}/pyrate-art-${DateTime.now().millisecondsSinceEpoch}');
      await f.writeAsBytes(bytes);
      await _setArt(station, kind, f.path);
      await f.delete();
    } catch (e) {
      setState(() => artNote = 'Could not fetch that link');
    }
  }

  List<Widget> _art(String id, Map mine) => [
        Text('THE ART', style: carved(14)),
        const SizedBox(height: 8),
        Text('A cover for the Log and the dial, a banner across the top of your page. Any picture works: it is redrawn on this phone before it sails, stripped of where and when it was taken. Album covers inside your recordings show while they play.',
            style: serif(14, color: boneFaint, italic: true)),
        const SizedBox(height: 14),
        if (mine['banner'] != null)
          Padding(
            padding: const EdgeInsets.only(bottom: 12),
            child: StationArt(station: id, hash: mine['banner'], width: double.infinity, height: 96, fallback: const SizedBox()),
          ),
        Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Container(
            decoration: BoxDecoration(border: Border.all(color: boneGhost, width: 0.5)),
            child: StationArt(station: id, hash: mine['cover'], width: 96, height: 96,
                fallback: Text('NO COVER', style: carved(9, color: boneGhost))),
          ),
          const SizedBox(width: 16),
          Expanded(
            child: Wrap(spacing: 10, runSpacing: 10, children: [
              button('COVER', () => pickArt(id, 'cover')),
              button('BANNER', () => pickArt(id, 'banner')),
              button('COVER FROM A LINK', () => linkArt(id, 'cover')),
              if (mine['cover'] != null || mine['banner'] != null)
                button('TAKE IT DOWN', () {
                  Native.call({'cmd': 'art_clear', 'station': id, 'kind': 'cover'});
                  Native.call({'cmd': 'art_clear', 'station': id, 'kind': 'banner'});
                }, color: bloodBright),
            ]),
          ),
        ]),
        if (artNote != null) Padding(padding: const EdgeInsets.only(top: 10), child: Text(artNote!, style: serif(13, color: boneFaint, italic: true))),
      ];

  Future<void> add(String station) async {
    final picked = await FilePicker.platform.pickFiles(type: FileType.audio, allowMultiple: true);
    if (picked == null) return;
    for (final f in picked.files) {
      if (f.path == null) continue;
      final stem = f.name.replaceAll(RegExp(r'\.[^.]+$'), '');
      final parts = stem.split(' - ');
      Native.call({
        'cmd': 'add_track',
        'station': station,
        'path': f.path,
        'title': parts.length > 1 ? parts.sublist(1).join(' - ') : stem,
        'by': parts.length > 1 ? parts.first : '',
        'talk': talk,
      });
    }
  }

  void loadLibrary(String station, int count) {
    if (libraryFor == station && libraryCount == count) return;
    libraryFor = station;
    libraryCount = count;
    final l = Native.call({'cmd': 'library', 'station': station});
    library = l is List ? l : [];
  }

  @override
  Widget build(BuildContext context) {
    final mine = widget.radio.mine;
    return ListView(padding: const EdgeInsets.fromLTRB(24, 26, 24, 40), children: [
      if (mine == null) ...[
        Text('RAISE COLOURS', style: carved(22)),
        const SizedBox(height: 10),
        Text('A station is a key made on this phone. Name it, put recordings on its clock, and it plays all day and all night, carried by every listener.',
            style: serif(16, color: boneFaint, italic: true)),
        const SizedBox(height: 22),
        TextField(controller: name, style: serif(20), decoration: field('Name'), maxLength: 40),
        TextField(controller: about, style: serif(16), decoration: field('What it broadcasts'), maxLength: 140),
        const SizedBox(height: 18),
        StartShapePicker(shape: shape),
        const SizedBox(height: 10),
        Text('All of it can change later, and none of it needs you once it is set: the station keeps to it for as long as anyone carries it.',
            style: serif(13, color: boneGhost, italic: true)),
        const SizedBox(height: 18),
        Row(children: [button(raising ? 'RAISING' : 'RAISE COLOURS', raising ? null : raise)]),
        if (error != null) Padding(padding: const EdgeInsets.only(top: 12), child: Text(error!, style: serif(14, color: bloodBright))),
        const SizedBox(height: 28),
        Text('Your phone hands its broadcasts to the network along a chain of other nodes, so the first node to show them is never yours. Listening goes through two hops: one sees who, the other sees what, neither sees both.',
            style: serif(13, color: boneGhost, italic: true)),
        const SizedBox(height: 34),
        Text('TAKE BACK A STATION', style: carved(14)),
        const SizedBox(height: 8),
        Text('Lost the phone, or had it taken? The 24 words bring the station to this phone and shut the old one out.',
            style: serif(14, color: boneFaint, italic: true)),
        TextField(controller: restoreWords, style: mono(12, color: bone), maxLines: 3, decoration: field('The 24 words')),
        const SizedBox(height: 12),
        Row(children: [
          button('TAKE IT BACK', () async {
            final r = await Native.callAway({'cmd': 'restore', 'phrase': restoreWords.text});
            setState(() => error = (r is Map && r['error'] != null) ? r['error'] : null);
          }),
        ]),
        const SizedBox(height: 34),
        const HandsPanel(),
      ] else
        ..._station(mine),
      ..._chart(),
    ]);
  }

  List<Widget> _chart() {
    final all = Native.call({'cmd': 'stations', 'marooned': true});
    final marooned = all is List ? all.where((x) => x['marooned'] == true).toList() : [];
    return [
      const SizedBox(height: 34),
      if (marooned.isNotEmpty) ...[
        Text('MAROONED', style: carved(14)),
        const SizedBox(height: 6),
        for (final m in marooned)
          Row(children: [
            Expanded(child: Text(m['name'] ?? '', style: serif(16, color: boneDim))),
            TextButton(
              onPressed: () => setState(() => Native.call({'cmd': 'maroon', 'station': m['id'], 'value': false})),
              child: Text('BRING BACK', style: carved(10, color: boneFaint)),
            ),
          ]),
        const SizedBox(height: 26),
      ],
      Text('ART', style: carved(14)),
      const SizedBox(height: 8),
      Text('Pictures you see while browsing live in memory and are gone when the app closes. Only the art of stations you keep or listen to is kept, sealed like the recordings.',
          style: serif(14, color: boneFaint, italic: true)),
      Row(children: [
        Switch(
          value: (Native.call({'cmd': 'art_kept_only'}) as Map)['value'] == true,
          onChanged: (v) => setState(() => Native.call({'cmd': 'art_kept_only', 'value': v})),
          activeThumbColor: bone, activeTrackColor: boneGhost, inactiveThumbColor: boneFaint, inactiveTrackColor: panel,
        ),
        Flexible(child: Text('Show art only for stations I keep', style: mono(11))),
      ]),
      const SizedBox(height: 26),
      Text('MATURE AND EXPLICIT', style: carved(14)),
      const SizedBox(height: 8),
      Text('Each station says for itself whether it is mature or explicit. A station that lies about it is yours to maroon.',
          style: serif(14, color: boneFaint, italic: true)),
      Row(children: [
        Switch(
          value: pref('hide_mature') == true,
          onChanged: (v) => setState(() => pref('hide_mature', v)),
          activeThumbColor: bone, activeTrackColor: boneGhost, inactiveThumbColor: boneFaint, inactiveTrackColor: panel,
        ),
        Flexible(child: Text('Hide mature stations', style: mono(11))),
      ]),
      Row(children: [
        Switch(
          value: pref('show_explicit') == true,
          onChanged: (v) => setState(() => pref('show_explicit', v)),
          activeThumbColor: bone, activeTrackColor: boneGhost, inactiveThumbColor: boneFaint, inactiveTrackColor: panel,
        ),
        Flexible(child: Text('Show explicit stations', style: mono(11))),
      ]),
      const SizedBox(height: 26),
      ShipPapers(radio: widget.radio),
      const SizedBox(height: 26),
      Text('THE HOLD', style: carved(14)),
      const SizedBox(height: 8),
      Text('What you listen to is kept here so your phone can play it without waiting. It is sealed: nothing on the phone says what any of it is. Clearing it leaves your own station and the ballast you carry for the fleet.',
          style: serif(14, color: boneFaint, italic: true)),
      const SizedBox(height: 10),
      Row(children: [
        button('CLEAR THE HOLD', () {
          final r = Native.call({'cmd': 'clear_hold'});
          setState(() => error = (r is Map && r['error'] != null) ? r['error'] : null);
        }),
      ]),
    ];
  }

  List<Widget> _onAir(String id) {
    final live = widget.radio.status['live'] as Map<String, dynamic>?;
    final on = live != null && live['station'] == id;
    return [
      Text('ON AIR', style: carved(14, color: bloodBright)),
      const SizedBox(height: 8),
      Text('Go live from this phone. Listeners hear you ten to twenty seconds behind, and when you stop, the clock takes back the hour.',
          style: serif(14, color: boneFaint, italic: true)),
      const SizedBox(height: 12),
      if (on)
        Row(children: [
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
            decoration: BoxDecoration(border: Border.all(color: bloodBright, width: 0.8)),
            child: Text('ON AIR  ${fmtTime((live['since_ms'] as num).toInt())}', style: mono(13, color: bloodBright, weight: FontWeight.w700)),
          ),
          const SizedBox(width: 12),
          button('END BROADCAST', () {
            Native.call({'cmd': 'end_live'});
            _service.invokeMethod('live', {'on': false}).catchError((_) {});
          }),
        ])
      else
        Row(children: [
          button('GO ON AIR', () async {
            final ok = await _service.invokeMethod<bool>('mic').catchError((_) => false) ?? false;
            if (!ok) {
              setState(() => error = 'Allow the microphone, then press again.');
              return;
            }
            final r = Native.call({'cmd': 'go_live', 'station': id});
            if (r is Map && r['error'] != null) {
              setState(() => error = r['error']);
            } else {
              _service.invokeMethod('live', {'on': true}).catchError((_) {});
            }
          }, color: bloodBright),
        ]),
      if (error != null) Padding(padding: const EdgeInsets.only(top: 10), child: Text(error!, style: serif(14, color: bloodBright))),
    ];
  }

  List<Widget> _programme(String id, Map<String, dynamic> mine) {
    if (programmeFor != id) {
      programmeFor = id;
      final r = Native.call({'cmd': 'programme', 'station': id});
      programme = r is Map ? r['programme'] as Map? : null;
    }
    Future<void> after(Future<void> f) async {
      await f;
      setState(() => programmeFor = null);
    }

    return [
      Text('THE PROGRAMME', style: carved(14)),
      const SizedBox(height: 8),
      Text(programmeSummary(programme), style: serif(14, color: boneFaint, italic: true)),
      const SizedBox(height: 12),
      Wrap(spacing: 6, runSpacing: 6, children: [
        button('SET THE PROGRAMME', () => after(Navigator.push(context, MaterialPageRoute(builder: (_) => Scaffold(backgroundColor: black, appBar: AppBar(backgroundColor: black), body: ProgrammeEditor(station: id, name: (mine['name'] ?? '').toString())))))),
        button('A PREMIERE', () => after(pyrateSheet(context, ShowEditor(station: id)))),
        button('A NOTICE', () => after(pyrateSheet(context, NoticeEditor(station: id)))),
        button('YOUR PAGE', () => openPage(context, mine, widget.radio)),
        button('A POSTER', () => pyrateSheet(context, PosterSheet(station: mine))),
        button('NUMBERS', () => after(pyrateSheet(context, NumbersKit(station: id)))),
      ]),
    ];
  }

  List<Widget> _station(Map<String, dynamic> mine) {
    final id = mine['id'] as String;
    loadLibrary(id, (mine['tracks'] as num?)?.toInt() ?? 0);
    final jobs = widget.radio.jobs;
    return [
      Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Flag(id: id, width: 96),
        const SizedBox(width: 18),
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(mine['name'] ?? '', style: serif(26)),
            Text(mine['call_sign'] ?? '', style: mono(12)),
            const SizedBox(height: 6),
            Text('${mine['tracks']} ${mine['tracks'] == 1 ? 'recording' : 'recordings'} on the clock', style: serif(14, color: boneFaint, italic: true)),
            if (mine['direct'] == true)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Text('No ship would take your station along the stem, so it is on the air straight from this phone. Listeners\' nodes can see this phone\'s address until the fleet grows.',
                    style: serif(13, color: brass, italic: true)),
              ),
          ]),
        ),
      ]),
      const SizedBox(height: 18),
      AboardChart(station: id),
      const SizedBox(height: 26),
      ..._art(id, mine),
      const SizedBox(height: 26),
      DetailsPanel(station: id),
      const SizedBox(height: 26),
      ..._onAir(id),
      const SizedBox(height: 26),
      RecordPanel(station: id),
      const SizedBox(height: 26),
      Text('ADD RECORDINGS', style: carved(14)),
      const SizedBox(height: 8),
      Text('Recordings join the library, and the programme below decides when each plays. Name files "Artist - Title" and the clock shows both.',
          style: serif(14, color: boneFaint, italic: true)),
      Row(children: [
        Switch(value: talk, onChanged: (v) => setState(() => talk = v), activeThumbColor: bone, activeTrackColor: boneGhost, inactiveThumbColor: boneFaint, inactiveTrackColor: panel),
        Text(talk ? 'Talk · mono · 24 kbit/s' : 'Music · stereo · 64 kbit/s', style: mono(11)),
      ]),
      Row(children: [button('CHOOSE FILES', () => add(id))]),
      for (final j in jobs)
        Padding(
          padding: const EdgeInsets.only(top: 10),
          child: Row(children: [
            Expanded(child: Text(j['state'] ?? '', style: serif(14, color: boneDim, italic: true))),
            if (!(j['state'] as String).startsWith('Setting'))
              IconButton(icon: const Icon(Icons.close, size: 16, color: boneGhost), onPressed: () => Native.call({'cmd': 'clear_job', 'job': j['id']})),
          ]),
        ),
      const SizedBox(height: 26),
      if (library.isNotEmpty) ...[
        Text('THE LIBRARY', style: carved(14)),
        const SizedBox(height: 6),
        for (final t in library)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 6),
            child: Row(children: [
              Expanded(
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  Text(t['title'] ?? '', maxLines: 1, overflow: TextOverflow.ellipsis, style: serif(16, color: boneDim)),
                  if ((t['by'] ?? '').toString().isNotEmpty) Text(t['by'], style: carved(9, color: boneFaint, spacing: 0.2)),
                ]),
              ),
              Text(fmtTime((t['duration_ms'] as num).toInt()), style: mono(11, color: boneGhost)),
              IconButton(
                icon: const Icon(Icons.remove, size: 16, color: boneGhost),
                tooltip: 'Withdraw',
                onPressed: () {
                  Native.call({'cmd': 'withdraw', 'station': id, 'track': t['hash']});
                  libraryCount = -1;
                },
              ),
            ]),
          ),
        const SizedBox(height: 26),
      ],
      ..._programme(id, mine),
      const SizedBox(height: 26),
      FeedsPanel(station: id),
      const SizedBox(height: 26),
      HandsPanel(station: id),
      const SizedBox(height: 26),
      Text('THE KEY', style: carved(14)),
      const SizedBox(height: 8),
      Text('This phone holds a deputy key: it broadcasts, and that is all it can do. The 24 words you wrote down are the station. With them, any phone can take the station back and shut this one out, or strike its colours.',
          style: serif(14, color: boneFaint, italic: true)),
      const SizedBox(height: 34),
      Text('STRIKE COLOURS', style: carved(14, color: bloodBright)),
      const SizedBox(height: 8),
      Text('Ends the station. Every honest node drops its recordings. It takes the 24 words.',
          style: serif(14, color: boneFaint, italic: true)),
      const SizedBox(height: 10),
      Row(children: [
        button('STRIKE COLOURS', () async {
          final words = TextEditingController();
          final ok = await showDialog<bool>(
            context: context,
            builder: (c) => AlertDialog(
              backgroundColor: panel,
              shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
              title: Text('Strike colours?', style: serif(22)),
              content: Column(mainAxisSize: MainAxisSize.min, children: [
                Text('The station ends for good. This cannot be undone.', style: serif(15, color: boneFaint)),
                TextField(controller: words, style: mono(12, color: bone), maxLines: 3, decoration: field('The 24 words')),
              ]),
              actions: [
                TextButton(onPressed: () => Navigator.pop(c, false), child: Text('KEEP FLYING', style: carved(11))),
                TextButton(onPressed: () => Navigator.pop(c, true), child: Text('STRIKE', style: carved(11, color: bloodBright))),
              ],
            ),
          );
          if (ok == true) {
            final r = Native.call({'cmd': 'strike', 'station': id, 'phrase': words.text});
            if (r is Map && r['error'] != null) setState(() => error = r['error']);
          }
        }, color: bloodBright),
      ]),
    ];
  }
}
