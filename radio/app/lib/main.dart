// Pyrate Radio. No rights reserved.

import 'dart:async';
import 'dart:io';
import 'dart:math' as math;

import 'shell.dart';
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
const bloodBright = Color(0xFFED5555);
// The site's accent red, as every HOSTIS page uses it: the lead control, the divider's diamond,
// the mark beside what is tuned. Bright red stays for the open air alone.
const red = Color(0xFFC52828);
const line = Color(0x24EDE4CE);
const lineSoft = Color(0x12EDE4CE);

TextStyle carved(double size, {Color color = bone, double spacing = 0.3}) =>
    TextStyle(fontFamily: 'Cinzel', fontSize: size, color: color, letterSpacing: size * spacing, height: 1.2);
TextStyle serif(double size, {Color color = bone, bool italic = false}) =>
    TextStyle(fontFamily: 'Fell', fontSize: size, color: color, fontStyle: italic ? FontStyle.italic : FontStyle.normal, height: 1.35);
/// IM Fell DW Pica SC, the face of every HOSTIS masthead and title.
TextStyle pica(double size, {Color color = bone, double spacing = 0.04}) =>
    TextStyle(fontFamily: 'Pica', fontSize: size, color: color, letterSpacing: size * spacing, height: 1.12);
/// A mono caption in capitals, the way the site labels its controls and readings.
TextStyle caps(double size, {Color color = boneFaint, double spacing = 0.2}) =>
    TextStyle(fontFamily: 'Mono', fontSize: size, color: color, letterSpacing: size * spacing);
TextStyle mono(double size, {Color color = boneFaint, FontWeight weight = FontWeight.w400}) =>
    TextStyle(fontFamily: 'Mono', fontSize: size, color: color, letterSpacing: size * 0.05, fontWeight: weight);

const _service = ShellChannel();

/// The flagship bar every HOSTIS page opens with: the skull and HOSTIS in Pica capitals.
class HostisBar extends StatelessWidget {
  const HostisBar({super.key});
  @override
  Widget build(BuildContext context) => Container(
        height: 46,
        decoration: const BoxDecoration(border: Border(bottom: BorderSide(color: lineSoft, width: 1))),
        child: Semantics(
          label: 'HOSTIS',
          child: ExcludeSemantics(
            child: Row(mainAxisAlignment: MainAxisAlignment.center, children: [
              const Text('\u2620\uFE0E', style: TextStyle(fontFamily: 'Skull', fontSize: 25, color: bone, height: 1)),
              const SizedBox(width: 11),
              Text('HOSTIS', style: pica(12.5, spacing: 0.3)),
            ]),
          ),
        ),
      );
}

/// The site's diamond: a small square turned on its point, in the accent red.
class Diamond extends StatelessWidget {
  final double size;
  final Color color;
  const Diamond({super.key, this.size = 7, this.color = red});
  @override
  Widget build(BuildContext context) => Transform.rotate(angle: math.pi / 4, child: SizedBox(width: size, height: size, child: ColoredBox(color: color)));
}

/// The divider of hostis.black: two hairlines fading out from a red diamond.
class HostisDivider extends StatelessWidget {
  final double width;
  const HostisDivider({super.key, this.width = 280});
  @override
  Widget build(BuildContext context) {
    Widget hair(bool left) => Expanded(
        child: Container(height: 1, decoration: BoxDecoration(gradient: LinearGradient(colors: left ? const [Color(0x00EDE4CE), Color(0x80EDE4CE)] : const [Color(0x80EDE4CE), Color(0x00EDE4CE)]))));
    return SizedBox(width: width, child: Row(children: [hair(true), const SizedBox(width: 18), const Diamond(), const SizedBox(width: 18), hair(false)]));
  }
}

/// A page's masthead: the title in Pica capitals and a line under it in italic.
Widget masthead(String title, {String? sub, CrossAxisAlignment align = CrossAxisAlignment.start}) => Column(crossAxisAlignment: align, children: [
      Text(title, style: pica(30, spacing: 0.07)),
      if (sub != null) ...[const SizedBox(height: 6), Text(sub, textAlign: align == CrossAxisAlignment.center ? TextAlign.center : TextAlign.start, style: serif(15, color: boneFaint, italic: true))],
    ]);

/// The one control on a screen that matters most, the way the site draws it: a red hairline
/// over a faint wash of red, the label in Pica.
class LeadButton extends StatelessWidget {
  final String label;
  final String? glyph;
  final VoidCallback? onPressed;
  final bool quiet;
  const LeadButton({super.key, required this.label, this.glyph, this.onPressed, this.quiet = false});
  @override
  Widget build(BuildContext context) {
    final on = onPressed != null;
    return Semantics(
      button: true,
      label: label,
      child: InkWell(
        onTap: onPressed,
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 180),
          padding: const EdgeInsets.symmetric(horizontal: 30, vertical: 13),
          decoration: BoxDecoration(
            color: quiet || !on ? Colors.transparent : const Color(0x14C52828),
            border: Border.all(color: !on ? boneGhost : quiet ? bone : red, width: 1),
          ),
          child: ExcludeSemantics(
            child: Row(mainAxisSize: MainAxisSize.min, children: [
              if (glyph != null) ...[Text(glyph!, style: TextStyle(fontFamily: 'Skull', fontSize: 13, height: 1, color: !on ? boneGhost : quiet ? bone : bloodBright)), const SizedBox(width: 12)],
              Text(label, style: pica(18, color: on ? bone : boneGhost, spacing: 0.06)),
            ]),
          ),
        ),
      ),
    );
  }
}

/// False when the node would not start: the dial says so rather than search for ever.
bool nodeStarted = true;

/// This build's version code, as the installed package says it, so it can never disagree with
/// the build. A release is offered only when its code is higher.
/// A day as the masthead writes it: 14 NOV.
String shortDate(int ms) {
  const m = ['JAN', 'FEB', 'MAR', 'APR', 'MAY', 'JUN', 'JUL', 'AUG', 'SEP', 'OCT', 'NOV', 'DEC'];
  final d = DateTime.fromMillisecondsSinceEpoch(ms);
  return '${d.day} ${m[d.month - 1]}';
}

Future<int> buildCode() async => await _service.invokeMethod<int>('build_code').catchError((_) => 0) ?? 0;

/// The stations a first open lands on when no station has yet earned it: HOSTIS's own.
List<String> harbour = [];

/// The open players this build knows, the first the one its links name.
List<String> openPlayers = [];

void main(List<String> args) async {
  launchArgs = args;
  WidgetsFlutterBinding.ensureInitialized();
  SystemChrome.setSystemUIOverlayStyle(const SystemUiOverlayStyle(
    statusBarColor: black,
    systemNavigationBarColor: black,
    statusBarIconBrightness: Brightness.light,
  ));
  final dir = await getApplicationSupportDirectory();
  await deskTidy(dir.path);
  // PYRATE_BOOTSTRAP names other first doors, for a private network or a test.
  final boot = Platform.environment['PYRATE_BOOTSTRAP'] ?? (await rootBundle.loadString('assets/bootstrap.txt')).trim();
  // HOSTIS's own stations, where a first open lands, and the keys that sign releases.
  harbour = (await rootBundle.loadString('assets/harbour.txt'))
      .split('\n')
      .map((l) => l.trim().split(RegExp(r'\s+')).first)
      .where((l) => RegExp(r'^[0-9a-f]{64}$').hasMatch(l))
      .toList();
  Native.setReleaseKeys(await rootBundle.loadString('assets/release_keys.txt'));
  // The hold key, from the phone's secure hardware: everything the node keeps is sealed under it.
  final hold = await _service.invokeMethod<String>('hold_key').catchError((_) => null);
  if (hold != null) Native.setHoldKey(hold);
  // Nothing about the node, the stations or the listener goes to the phone's log.
  nodeStarted = Native.start('${dir.path}${Platform.pathSeparator}node', boot) >= 0;
  // Every station that is not HOSTIS's own is passed on as an open player's link (IPFS), and so
  // are invitations and flocks; hostis.black carries HOSTIS's stations alone.
  openPlayers = (await rootBundle.loadString('assets/open_players.txt')).split('\n').map((l) => l.trim()).where((l) => l.startsWith('https://')).toList();
  if (nodeStarted && openPlayers.isNotEmpty) Native.call({'cmd': 'open_player', 'url': openPlayers.first});
  runApp(const PyrateApp());
}

/// Everything the screens show, polled from the node.
class Tuner extends ChangeNotifier {
  List<Map<String, dynamic>> stations = [];
  Map<String, dynamic> status = {};
  String? tuned;
  bool playing = false;
  bool scanning = false;
  /// The first open plays the busiest earned station once in the phone's life, never again.
  bool firstTuneDone = pref('first_tune') == true;

  /// A link from outside, waiting on the owner's yes: a station id, `pocket=…` or `flock=…`.
  String? invite;
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
  int carriedBytes = 0;
  Timer? _ballast;
  Timer? _update;
  Timer? _feeds;
  Map<String, dynamic>? update;

  Tuner() {
    _poll = Timer.periodic(const Duration(milliseconds: 700), (_) => _refresh());
    // Ballast fills only on Wi-Fi while charging, so the fleet never costs the owner.
    _ballast = Timer.periodic(const Duration(seconds: 60), (_) => _checkBallast());
    // A drawer Lantern picks up where it was left.
    final gb = (pref('lantern_gb') as num?)?.toInt() ?? 4;
    if (pref('lantern') == true) {
      Native.call({'cmd': 'ballast_size', 'gb': gb});
      _service.invokeMethod('lantern', {'on': true, 'gb': gb}).catchError((_) {});
    }
    _checkBallast();
    // A radio left playing comes back on where it was, the moment the app opens.
    final resume = pref('resume');
    if (firstTuneDone && resume is String && resume.length == 64 && Native.call({'cmd': 'status'})?['tuned'] == null) pendingLink = resume;
    unawaited(sweepTemp());
    unawaited(_refresh());
    // Podcast feeds are read every few hours, on Wi-Fi only, while the app is open.
    _feeds = Timer.periodic(const Duration(minutes: 20), (_) => _checkFeeds());
    Future.delayed(const Duration(seconds: 90), _checkFeeds);
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

  Future<void> _checkUpdate({bool now = false}) async {
    // Releases come down only on Wi-Fi or another unmetered network, unless asked for.
    final c = await _service.invokeMethod<Map>('conditions').catchError((_) => <dynamic, dynamic>{}) ?? {};
    if (c['unmetered'] != true && !now) return;
    final code = await buildCode();
    if (code == 0) return;
    final r = await Native.callAway({'cmd': 'update', 'code': code, 'now': now});
    if (r is Map && (r['available'] == true || r['coming'] is Map)) {
      update = r.cast<String, dynamic>();
      notifyListeners();
    }
  }

  /// A release seen less than three days ago, taken now at its listener's word.
  Future<void> takeUpdateNow() => _checkUpdate(now: true);

  Future<void> _checkFeeds() async {
    if (Feeds.load().isEmpty) return;
    final last = (pref('feeds_at') as num?)?.toInt() ?? 0;
    final now = DateTime.now().millisecondsSinceEpoch;
    if (now - last < 3 * 3600000) return;
    final c = await _service.invokeMethod<Map>('conditions').catchError((_) => <dynamic, dynamic>{}) ?? {};
    if (c['unmetered'] != true) return;
    pref('feeds_at', now);
    await Feeds.refresh();
  }

  Future<void> _checkBallast() async {
    final c = await _service.invokeMethod<Map>('conditions').catchError((_) => <dynamic, dynamic>{}) ?? {};
    final unmetered = c['unmetered'] == true;
    // "Keep only what I play": no ballast, and no serving of others' sealed chunks.
    final mine = pref('carry_mine_only') == true && pref('lantern') != true;
    final r = Native.call({'cmd': 'ballast_ok', 'value': !mine && unmetered && c['charging'] == true, 'unmetered': !mine && unmetered});
    if (r is Map && r['carrying'] is int) carrying = r['carrying'];
    if (r is Map && r['carried_bytes'] is int) carriedBytes = r['carried_bytes'];
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

  bool _refreshing = false;

  /// The dial's poll runs off the screen's thread, one at a time, so a node busy with a
  /// slow job never stalls a frame.
  Future<void> _refresh() async {
    if (_refreshing) return;
    _refreshing = true;
    try {
      await _refreshOnce();
    } finally {
      _refreshing = false;
    }
  }

  Future<void> _refreshOnce() async {
    _ticks++;
    final st = await Native.callAway({'cmd': 'status'});
    if (st is Map<String, dynamic> && st['error'] == null) status = st;
    // The lock screen, headphones or the car may have changed station or stopped the radio.
    final nodeTuned = status['tuned'] as String?;
    if (DateTime.now().millisecondsSinceEpoch - _tunedAt > 3000) {
      if (nodeTuned == null && playing) {
        playing = false;
      } else if (nodeTuned != null && nodeTuned != tuned) {
        tuned = nodeTuned;
        playing = true;
        face(nodeTuned);
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
        if (id == null) return;
        firstTuneDone = true;
        pref('first_tune', true);
        // A bell this phone rang itself tunes straight away.
        if (id.startsWith('tune=')) {
          pendingLink = id.substring(5);
          return;
        }
        // Anything from outside waits for a yes: a station is not played, a pocket or
        // a flock is not joined, until the owner says so on the dial.
        invite = id;
        if (!id.contains('=')) face(id);
        notifyListeners();
      }).catchError((_) {});
    }
    if (pendingLink != null && station(pendingLink) != null) {
      final id = pendingLink!;
      pendingLink = null;
      tune(id);
    }
    if (_ticks % 3 == 1 || stations.isEmpty) {
      final list = await Native.callAway({'cmd': 'stations'});
      if (list is List) {
        final at = pages.hasClients ? (pages.page ?? 0).round() : -1;
        final shown = at >= 0 && at < stations.length ? stations[at]['id'] as String? : null;
        final before = stations.map((s) => s['id']).join();
        stations = list.cast<Map<String, dynamic>>();
        // The dial follows the station, not its place in the list: when the Log reorders
        // or shrinks, the face turns back to what was tuned or shown, and tunes nothing.
        if (stations.map((s) => s['id']).join() != before) quiet = true;
        final want = tuned ?? shown;
        WidgetsBinding.instance.addPostFrameCallback((_) {
          face(want);
          WidgetsBinding.instance.addPostFrameCallback((_) => quiet = false);
        });
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
        pref('first_tune', true);
        tune((pick ?? stations.first)['id'] as String);
      }
    }
    // A window resized, or a phone turned, can leave the dial between two stations: it settles
    // back on the one tuned, or the nearest.
    if (pages.hasClients && stations.isNotEmpty && !pages.position.isScrollingNotifier.value) {
      final at = pages.page;
      if (at != null && (at - at.round()).abs() > 0.001) face(tuned ?? stations[at.round().clamp(0, stations.length - 1)]['id'] as String?);
    }
    notifyListeners();
  }

  String? pendingLink;

  void acceptInvite() {
    final id = invite;
    invite = null;
    if (id == null) return;
    if (id.startsWith('flock=')) {
      Native.callAway({'cmd': 'flock_join', 'link': id});
    } else if (id.startsWith('pocket=')) {
      // Joined here, and tuned once its log arrives.
      Native.callAway({'cmd': 'pocket_join', 'link': id}).then((r) {
        if (r is Map && r['station'] is String) pendingLink = r['station'] as String;
      });
    } else {
      pendingLink = id;
    }
    notifyListeners();
  }

  void declineInvite() {
    invite = null;
    notifyListeners();
  }

  void tune(String id) {
    tuned = id;
    playing = true;
    _tunedAt = DateTime.now().millisecondsSinceEpoch;
    _service.invokeMethod('remember', {'station': id}).catchError((_) {});
    pref('resume', id);
    Native.call({'cmd': 'tune', 'station': id});
    // The stations either side on the dial get their logs now, so a swipe plays at once.
    final i = stations.indexWhere((s) => s['id'] == id);
    if (i >= 0 && stations.length > 1) {
      final near = {stations[(i + 1) % stations.length]['id'], stations[(i - 1 + stations.length) % stations.length]['id']}..remove(id);
      Native.callAway({'cmd': 'warm', 'stations': near.toList()});
    }
    final name = station(id)?['name'] ?? 'Pyrate Radio';
    _service.invokeMethod('start', {'title': name}).catchError((_) {});
    face(id);
    notifyListeners();
  }

  /// While true the dial is being turned by the app, not by a hand, and turning it tunes nothing.
  bool quiet = false;

  /// Turns the dial to a station without tuning it.
  void face(String? id) {
    final i = stations.indexWhere((s) => s['id'] == id);
    if (i < 0 || !pages.hasClients || pages.position.isScrollingNotifier.value) return;
    final at = pages.page;
    if (at != null && (at - i).abs() < 0.001) return;
    quiet = true;
    pages.jumpToPage(i);
    WidgetsBinding.instance.addPostFrameCallback((_) => quiet = false);
  }

  void stop() {
    playing = false;
    _tunedAt = DateTime.now().millisecondsSinceEpoch;
    stopScan();
    pref('resume', '');
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
    _feeds?.cancel();
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
        colorScheme: const ColorScheme.dark(primary: bone, surface: black, onSurface: bone, secondary: bone, surfaceContainerHigh: panel, outline: boneGhost),
        textSelectionTheme: const TextSelectionThemeData(cursorColor: bone, selectionColor: Color(0x66C52828), selectionHandleColor: red),
        // The site's own sheet throughout: Fell for any text the app does not style itself,
        // square edges, hairline borders, nothing tinted or rounded by the toolkit's defaults.
        fontFamily: 'Fell',
        dialogTheme: const DialogThemeData(backgroundColor: panel, surfaceTintColor: Colors.transparent, shape: RoundedRectangleBorder(side: BorderSide(color: line, width: 1))),
        bottomSheetTheme: const BottomSheetThemeData(backgroundColor: panel, modalBackgroundColor: panel, surfaceTintColor: Colors.transparent, shape: RoundedRectangleBorder()),
        popupMenuTheme: const PopupMenuThemeData(color: panel, surfaceTintColor: Colors.transparent, shape: RoundedRectangleBorder(side: BorderSide(color: line, width: 1))),
        snackBarTheme: const SnackBarThemeData(backgroundColor: panel, behavior: SnackBarBehavior.floating, shape: RoundedRectangleBorder(side: BorderSide(color: line, width: 1))),
        timePickerTheme: const TimePickerThemeData(
          backgroundColor: panel,
          shape: RoundedRectangleBorder(side: BorderSide(color: line, width: 1)),
          hourMinuteShape: RoundedRectangleBorder(),
          dayPeriodShape: RoundedRectangleBorder(),
          dayPeriodBorderSide: BorderSide(color: boneGhost, width: 0.5),
        ),
        sliderTheme: const SliderThemeData(activeTrackColor: bone, inactiveTrackColor: boneGhost, thumbColor: bone, overlayColor: Colors.transparent, trackHeight: 1),
        progressIndicatorTheme: const ProgressIndicatorThemeData(color: boneFaint, linearTrackColor: boneGhost),
        checkboxTheme: CheckboxThemeData(
          shape: const RoundedRectangleBorder(),
          side: const BorderSide(color: boneFaint, width: 0.8),
          fillColor: WidgetStateProperty.resolveWith((st) => st.contains(WidgetState.selected) ? bone : Colors.transparent),
          checkColor: const WidgetStatePropertyAll(black),
        ),
        outlinedButtonTheme: OutlinedButtonThemeData(style: OutlinedButton.styleFrom(foregroundColor: bone, shape: const RoundedRectangleBorder(), side: const BorderSide(color: boneGhost, width: 1))),
        textButtonTheme: TextButtonThemeData(style: TextButton.styleFrom(foregroundColor: bone, shape: const RoundedRectangleBorder())),
        splashFactory: NoSplash.splashFactory,
        highlightColor: Colors.transparent,
      ),
      home: ListenableBuilder(
        listenable: radio,
        builder: (context, _) => Scaffold(
          // On a computer the set keeps a radio's width in the middle of the window.
          body: Center(child: ConstrainedBox(constraints: const BoxConstraints(maxWidth: 620), child: SafeArea(
            child: Column(children: [
              const HostisBar(),
              Expanded(child: IndexedStack(index: tab, children: [
              Dial(radio: radio),
              LogPage(radio: radio, onTune: () => setState(() => tab = 0)),
              PinGate(child: ColoursPage(radio: radio)),
              PinGate(child: SettingsPage(radio: radio)),
              ])),
            ]),
          ))),
          bottomNavigationBar: SafeArea(
            top: false,
            child: Container(
              decoration: const BoxDecoration(border: Border(top: BorderSide(color: line, width: 1))),
              height: 60,
              child: Row(children: [
                for (final (i, label) in ['THE DIAL', 'THE LOG', 'MY STATION', 'SETTINGS'].indexed)
                  Expanded(
                    child: Semantics(
                      selected: tab == i,
                      button: true,
                      child: InkWell(
                        onTap: () => setState(() => tab = i),
                        child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [
                          // The tab in hand wears the site's diamond.
                          AnimatedOpacity(opacity: tab == i ? 1 : 0, duration: const Duration(milliseconds: 160), child: const Diamond(size: 5)),
                          const SizedBox(height: 8),
                          Text(label, style: caps(10, color: tab == i ? bone : boneFaint, spacing: 0.22)),
                        ]),
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
      ..color = live ? bloodBright : boneGhost
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
    final inv = radio.invite;
    return Column(children: [
      if (inv != null)
        Container(
          width: double.infinity,
          padding: const EdgeInsets.fromLTRB(20, 10, 12, 6),
          decoration: const BoxDecoration(color: Color(0x0EC52828), border: Border(bottom: BorderSide(color: red, width: 1))),
          child: Row(children: [
            Expanded(
              child: Text(
                  inv.startsWith('pocket=')
                      ? 'AN INVITATION TO A PRIVATE STATION'
                      : inv.startsWith('flock=')
                          ? 'A FLOCK LINK · PLAY IN STEP WITH ITS HOST'
                          : 'A LINK TO ${((radio.station(inv)?['name'] ?? '').toString().trim().isEmpty ? 'A STATION' : radio.station(inv)!['name'].toString().trim()).toUpperCase()}',
                  style: caps(9.5, color: bone, spacing: 0.18)),
            ),
            TextButton(onPressed: radio.acceptInvite, child: Text(inv.contains('=') ? 'JOIN' : 'LISTEN', style: carved(10, color: bone))),
            TextButton(onPressed: radio.declineInvite, child: Text('NO', style: carved(10, color: boneFaint))),
          ]),
        ),
      if (up != null && up['coming'] is Map)
        InkWell(
          onTap: () => radio.takeUpdateNow(),
          child: Container(
            width: double.infinity,
            padding: const EdgeInsets.symmetric(vertical: 10, horizontal: 20),
            decoration: const BoxDecoration(border: Border(bottom: BorderSide(color: boneFaint, width: 1))),
            child: Text('${up['coming']['version']} ARRIVES ${shortDate((up['coming']['from_ms'] as num).toInt())} · TAP TO TAKE IT NOW', textAlign: TextAlign.center, style: carved(10, color: boneDim, spacing: 0.2)),
          ),
        ),
      if (up != null && up['available'] == true)
        InkWell(
          onTap: () => _service.invokeMethod('install', {'path': up['path']}).catchError((_) {}),
          child: Container(
            width: double.infinity,
            padding: const EdgeInsets.symmetric(vertical: 10, horizontal: 20),
            decoration: const BoxDecoration(color: Color(0x0EC52828), border: Border(bottom: BorderSide(color: red, width: 1))),
            child: Text('A NEW VERSION IS READY · ${up['version']} · TAP TO INSTALL', textAlign: TextAlign.center, style: carved(10, color: bone, spacing: 0.2)),
          ),
        ),
      const SizedBox(height: 16),
      Text('Pyrate Radio', style: pica(26, spacing: 0.09)),
      const SizedBox(height: 6),
      Row(mainAxisAlignment: MainAxisAlignment.center, children: [
        if (stations.isNotEmpty) ...[const Diamond(size: 4, color: boneFaint), const SizedBox(width: 9)],
        Text(stations.isEmpty ? 'ON THE AIR ALL DAY AND ALL NIGHT' : '${stations.length} ${stations.length == 1 ? 'STATION' : 'STATIONS'} ON THE AIR', style: caps(9.5, color: boneGhost)),
      ]),
      Expanded(
        child: stations.isEmpty
            ? Center(
                child: Column(mainAxisSize: MainAxisSize.min, children: [
                  if (!nodeStarted) ...[
                    Text('The radio would not start', style: serif(22, color: boneDim, italic: true)),
                    const SizedBox(height: 10),
                    Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 32),
                      child: Text(((Native.call({'cmd': 'status'}) as Map?)?['start_error'] ?? 'Close the app fully and open it again.').toString(),
                          textAlign: TextAlign.center, style: serif(13, color: boneFaint, italic: true)),
                    ),
                  ] else ...[
                    Text('Searching the Log', style: serif(22, color: boneDim, italic: true)),
                    const SizedBox(height: 10),
                    const SizedBox(width: 120, child: LinearProgressIndicator(minHeight: 1, color: boneFaint, backgroundColor: boneGhost)),
                  ],
                ]),
              )
            : GestureDetector(
                onLongPress: radio.startScan,
                onTap: radio.scanning ? radio.stopScan : null,
                child: PageView.builder(
                  controller: radio.pages,
                  itemCount: stations.length,
                  onPageChanged: (i) {
                    if (radio.quiet || i >= stations.length) return;
                    final id = stations[i]['id'] as String;
                    if (id != radio.tuned) radio.tune(id);
                  },
                  itemBuilder: (context, i) => _StationFace(radio: radio, s: stations[i]),
                ),
              ),
      ),
      const HostisDivider(width: 240),
      Padding(
        padding: const EdgeInsets.only(bottom: 16, top: 14),
        child: radio.scanning
            ? Padding(padding: const EdgeInsets.symmetric(vertical: 15), child: Text('SCANNING · TAP TO HOLD', style: caps(11, color: bone)))
            : Row(mainAxisAlignment: MainAxisAlignment.center, children: [
                LeadButton(
                  onPressed: stations.isEmpty ? null : radio.toggle,
                  quiet: radio.playing,
                  glyph: radio.playing ? '■︎' : '▶︎',
                  label: radio.playing ? 'Stop' : 'Listen',
                ),
              ]),
      ),
      Text('swipe to change station · hold to scan', style: serif(12, color: boneFaint, italic: true)),
      const SizedBox(height: 10),
    ]);
  }
}

/// What a station offers beyond the page and passing it on, one tap down from the dial.
class _StationMore extends StatelessWidget {
  final Tuner radio;
  final Map<String, dynamic> s;
  final bool tuned;
  const _StationMore({required this.radio, required this.s, required this.tuned});
  @override
  Widget build(BuildContext context) {
    Widget row(String label, String about, VoidCallback act) => InkWell(
          onTap: () {
            Navigator.pop(context);
            act();
          },
          child: Container(
            width: double.infinity,
            padding: const EdgeInsets.symmetric(vertical: 14),
            decoration: const BoxDecoration(border: Border(bottom: BorderSide(color: lineSoft))),
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(label, style: carved(13)),
              const SizedBox(height: 3),
              Text(about, style: serif(14, color: boneFaint, italic: true)),
            ]),
          ),
        );
    return Padding(
      padding: const EdgeInsets.fromLTRB(24, 22, 24, 30),
      child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text((s['name'] ?? '').toString(), style: pica(24)),
        const SizedBox(height: 10),
        row('SCHEDULE', 'What plays today and this week, with a reminder for any of it.', () => pyrateSheet(context, WatchBill(station: s))),
        if (tuned) row('RECORD THIS', 'Save what is playing to this device.', () => tapeIt(context)),
        row('PLAY ON SEVERAL PHONES', 'Phones in one room playing in step, like one speaker.', () => pyrateSheet(context, const FlockSheet())),
        row('SLEEP TIMER', radio.sleepAt == null ? 'Fade out and stop after a while.' : 'Fades out at ${clockTime(radio.sleepAt!.millisecondsSinceEpoch)}.', () => sleepSheet(context, radio)),
        row('RECENTLY PLAYED', 'What has played since the app opened.', () => historySheet(context, radio)),
      ]),
    );
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
      Positioned.fill(child: Padding(
      padding: const EdgeInsets.symmetric(horizontal: 28),
      child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [
        pic == null
            ? Flag(id: id, width: 200, live: live)
            : Container(
                decoration: BoxDecoration(border: Border.all(color: accent == bone ? line : accent.withValues(alpha: 0.45), width: 1), boxShadow: const [BoxShadow(color: Color(0xB3000000), blurRadius: 40, offset: Offset(0, 18))]),
                child: StationArt(station: id, hash: pic, width: 220, height: 220, fallback: Flag(id: id, width: 180, live: live)),
              ),
        const SizedBox(height: 26),
        if (live)
          Container(
            margin: const EdgeInsets.only(bottom: 12),
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 3),
            decoration: BoxDecoration(border: Border.all(color: bloodBright, width: 1)),
            child: Row(mainAxisSize: MainAxisSize.min, children: [
              Container(width: 6, height: 6, decoration: const BoxDecoration(color: bloodBright, shape: BoxShape.circle)),
              const SizedBox(width: 8),
              Text('ON AIR', style: caps(10, color: bloodBright, spacing: 0.3)),
            ]),
          ),
        GestureDetector(
          onTap: () => openPage(context, s, radio),
          child: Row(mainAxisSize: MainAxisSize.min, children: [
            Flexible(child: Text(s['name'] ?? '', textAlign: TextAlign.center, style: pica(32, spacing: 0.03))),
            ...seals(s, size: 10),
          ]),
        ),
        const SizedBox(height: 6),
        Row(mainAxisSize: MainAxisSize.min, children: [
          Text(frequency(id).toStringAsFixed(1), style: mono(15, color: bone)),
          const SizedBox(width: 10),
          Text(s['call_sign'] ?? '', style: caps(11, color: boneFaint, spacing: 0.14)),
        ]),
        const SizedBox(height: 8),
        Wrap(alignment: WrapAlignment.center, crossAxisAlignment: WrapCrossAlignment.center, spacing: 2, children: [
          for (final (label, act) in <(String, VoidCallback)>[
            ('THE PAGE', () => openPage(context, s, radio)),
            if (s['pocket'] != true) ('PASS IT ON', () => shareStation(s)),
            if ((s['tip'] ?? '').toString().isNotEmpty) ('TIP', () => tipStation(context, s['tip'], name: (s['name'] ?? 'the station').toString())),
          ])
            TextButton(
              onPressed: act,
              style: TextButton.styleFrom(padding: const EdgeInsets.symmetric(horizontal: 10), minimumSize: const Size(0, 40)),
              child: Text(label, style: caps(9.5, color: boneFaint, spacing: 0.16)),
            ),
          IconButton(
            tooltip: 'More for this station',
            icon: Icon(Icons.more_horiz, size: 20, color: radio.sleepAt == null ? boneFaint : bone),
            onPressed: () => pyrateSheet(context, _StationMore(radio: radio, s: s, tuned: isTuned)),
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
                  ? Text('Tuning in', style: serif(16, color: boneFaint, italic: true))
                  : state == 'dead air'
                      ? Text('Dead air', style: serif(16, color: boneFaint, italic: true))
                      : state == 'off air'
                      ? Text(offAirLabel((now?['back_at'] as num?)?.toInt()), textAlign: TextAlign.center, style: serif(16, color: boneFaint, italic: true))
                      : Column(children: [
                          if (state != 'live' && (now?['slot'] ?? '').toString().isNotEmpty)
                            Text((now!['slot']).toString().toUpperCase(), maxLines: 1, overflow: TextOverflow.ellipsis, style: caps(9.5, color: accent, spacing: 0.26)),
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
        Text('${s['aboard'] ?? 0} LISTENING', style: caps(11, color: boneFaint, spacing: 0.2)),
      ]),
      )),
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
  final restorePass = TextEditingController();
  final pass = TextEditingController();
  bool raising = false;
  bool talk = false;
  String? error;
  List library = [];
  String? libraryFor;
  int libraryCount = -1;
  final shape = StartShape();
  bool pocket = false;
  Map? programme;
  String? programmeFor;

  InputDecoration field(String label) => InputDecoration(
        labelText: label,
        labelStyle: serif(15, color: boneFaint, italic: true),
        enabledBorder: const UnderlineInputBorder(borderSide: BorderSide(color: boneGhost)),
        focusedBorder: const UnderlineInputBorder(borderSide: BorderSide(color: bone)),
      );
  @override
  void dispose() {
    name.dispose();
    about.dispose();
    restoreWords.dispose();
    restorePass.dispose();
    pass.dispose();
    super.dispose();
  }


  Widget button(String label, VoidCallback? onTap, {Color color = bone}) => OutlinedButton(
        onPressed: onTap,
        style: OutlinedButton.styleFrom(
          side: BorderSide(color: onTap == null ? lineSoft : color.withValues(alpha: 0.42), width: 1),
          shape: const RoundedRectangleBorder(),
          padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 15),
        ),
        child: Text(label, style: caps(10.5, color: onTap == null ? boneGhost : color, spacing: 0.15)),
      );

  Future<void> raise() async {
    setState(() {
      raising = true;
      error = null;
    });
    final withPass = pass.text.isNotEmpty;
    final r = await Native.callAway({'cmd': 'raise', 'name': name.text, 'about': about.text, 'pocket': pocket, 'passphrase': pass.text});
    pass.clear();
    // The words first: they are shown once, so nothing that can fail comes before them, and
    // neither the back gesture nor a stray tap closes them before the keeper says so.
    if (r is Map && r['words'] is String && mounted) {
      var written = false;
      await secure(true);
      if (!mounted) return;
      await showDialog(
        context: context,
        barrierDismissible: false,
        builder: (c) => PopScope(
          canPop: false,
          child: StatefulBuilder(
            builder: (c, set) => AlertDialog(
              backgroundColor: panel,
              shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
              title: Text('Write these down', style: serif(24)),
              content: SingleChildScrollView(
                child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
                  Text('These 24 words are your station. If this phone is lost or taken, they bring the station back on any phone and lock this one out. They are shown once and saved nowhere, so write them on paper. Nobody from Pyrate Radio or HOSTIS will ever ask for them.',
                      style: serif(15, color: boneFaint, italic: true)),
                  const SizedBox(height: 14),
                  Container(
                    padding: const EdgeInsets.all(12),
                    decoration: BoxDecoration(color: black, border: Border.all(color: boneGhost, width: 0.5)),
                    child: SelectableText(r['words'], style: mono(13, color: bone)),
                  ),
                  if (withPass) Padding(
                    padding: const EdgeInsets.only(top: 10),
                    child: Text('Keep your passphrase apart from the words. Without it, these words open a different station.', style: serif(14, color: boneFaint, italic: true)),
                  ),
                  CheckboxListTile(
                    value: written,
                    onChanged: (v) => set(() => written = v == true),
                    contentPadding: EdgeInsets.zero,
                    controlAffinity: ListTileControlAffinity.leading,
                    title: Text('I have written all 24 down, on paper', style: serif(15)),
                  ),
                ]),
              ),
              actions: [TextButton(onPressed: written ? () => Navigator.pop(c) : null, child: Text('THEY ARE WRITTEN DOWN', style: carved(11, color: written ? bone : boneGhost)))],
            ),
          ),
        ),
      );
      await secure(false);
    }
    String? shaped;
    if (r is Map && r['id'] is String) {
      try {
        await shape.apply(r['id']);
      } catch (e) {
        shaped = 'The station is up, but its first settings did not take: $e';
      }
    }
    if (!mounted) return;
    setState(() {
      raising = false;
      if (r is Map && r['error'] != null) error = r['error'];
      if (r is Map && r['pocket_error'] != null) error = 'The station is private, but its topic did not open yet: ${r['pocket_error']}';
      if (shaped != null) error = shaped;
    });
  }

  String? artNote;

  Future<void> _setArt(String station, String kind, String path) async {
    setState(() => artNote = 'Redrawing the picture');
    final r = await Native.callAway({'cmd': 'art_set', 'station': station, 'kind': kind, 'path': path});
    if (mounted) setState(() => artNote = r is Map && r['error'] != null ? 'That picture would not take: ${r['error']}' : null);
  }

  Future<void> pickArt(String station, String kind) async {
    final picked = await FilePicker.platform.pickFiles(type: FileType.image);
    final path = picked?.files.single.path;
    if (path != null) {
      await _setArt(station, kind, path);
      unawaited(dropCopy(path));
    }
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
          TextField(enableIMEPersonalizedLearning: false, controller: link, autofocus: true, style: mono(12, color: bone), decoration: field('https://')),
        ]),
        actions: [TextButton(onPressed: () => Navigator.pop(c, link.text.trim()), child: Text('FETCH', style: carved(11)))],
      ),
    );
    if (url == null || !url.startsWith('http')) return;
    setState(() => artNote = 'Fetching the picture');
    final path = await fetchOut(url);
    if (path == null) {
      if (mounted) setState(() => artNote = 'Could not fetch that link');
      return;
    }
    try {
      await _setArt(station, kind, path);
    } finally {
      unawaited(File(path).delete().then((_) {}, onError: (_) {}));
    }
  }

  List<Widget> _art(String id, Map mine) => [
        heading('PICTURES'),
        note('A cover for the dial and the Log, and a banner for your station\'s page. Where and when a photo was taken is wiped before it goes out.'),
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
              button('FROM A LINK', () => linkArt(id, 'cover')),
              if (mine['cover'] != null || mine['banner'] != null)
                button('REMOVE', () async {
                  await Native.callAway({'cmd': 'art_clear', 'station': id, 'kind': 'cover'});
                  await Native.callAway({'cmd': 'art_clear', 'station': id, 'kind': 'banner'});
                }, color: boneFaint),
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
      // One at a time, and the picker's copy goes once each is set.
      setTrack({
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
        masthead('Start a station'),
        const SizedBox(height: 10),
        note('Give it a name, add some recordings, and it plays day and night. Nobody can take it down.'),
        const SizedBox(height: 18),
        TextField(enableIMEPersonalizedLearning: false, controller: name, style: serif(20), decoration: field('Station name'), maxLength: 40),
        TextField(enableIMEPersonalizedLearning: false, controller: about, style: serif(16), decoration: field('What it plays, in a line'), maxLength: 140),
        const SizedBox(height: 14),
        StartShapePicker(shape: shape),
        const SizedBox(height: 8),
        fold('More options', sub: 'Invite-only, and an extra passphrase', [
          setting('Invite-only', pocket, (v) => setState(() => pocket = v), sub: 'Hidden from the Log. Only people you send an invitation can find it or hear it, and they can see each other\'s devices.'),
          const SizedBox(height: 10),
          note('A passphrase adds a lock to your 24 words. Someone who sees the words still cannot open the station. Forget the passphrase and the station is gone for good.'),
          TextField(controller: pass, obscureText: true, style: mono(13, color: bone), decoration: field('Passphrase (optional)'),
              enableSuggestions: false, autocorrect: false, enableIMEPersonalizedLearning: false),
        ]),
        const SizedBox(height: 14),
        Row(children: [LeadButton(label: raising ? 'Starting' : 'Raise colours', onPressed: raising ? null : raise)]),
        const SizedBox(height: 8),
        note('You can change all of this later.'),
        if (error != null) Padding(padding: const EdgeInsets.only(top: 12), child: Text(error!, style: serif(14, color: bone, italic: true))),
        const SizedBox(height: 30),
        fold('Bring back a station', sub: 'Have your 24 words? Move your station to this phone.', [
          note('This also locks out the old phone, if it was lost or taken.'),
          TextField(controller: restoreWords, style: mono(12, color: bone), maxLines: 3, decoration: field('The 24 words'),
              enableSuggestions: false, autocorrect: false, enableIMEPersonalizedLearning: false, keyboardType: TextInputType.visiblePassword),
          TextField(controller: restorePass, obscureText: true, style: mono(12, color: bone), decoration: field('Passphrase, if you set one'),
              enableSuggestions: false, autocorrect: false, enableIMEPersonalizedLearning: false),
          const SizedBox(height: 12),
          Row(children: [
            button('BRING IT BACK', () async {
              final phrase = restoreWords.text, pp = restorePass.text;
              restoreWords.clear();
              restorePass.clear();
              final r = await Native.callAway({'cmd': 'restore', 'phrase': phrase, 'passphrase': pp});
              if (!context.mounted) return;
              toast(context, r is Map && r['error'] != null ? 'That did not work: ${r['error']}' : 'Your station is on this phone, and the old phone is locked out.');
            }),
          ]),
        ]),
        fold('Co-host someone else\'s station', sub: 'Broadcast for a friend\'s station from this phone.', [const HandsPanel()]),
      ] else
        ..._station(mine),
    ]);
  }

  List<Widget> _onAir(String id) {
    final live = widget.radio.status['live'] as Map<String, dynamic>?;
    final on = live != null && live['station'] == id;
    return [
      heading('GO LIVE', color: bloodBright),
      note('Talk to your listeners from this phone. They hear you about fifteen seconds behind, and your recordings carry on when you stop. Sirens, bells or voices in the background can give away where you are. Settings can disguise your voice.'),
      const SizedBox(height: 12),
      if (on)
        Row(children: [
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
            decoration: BoxDecoration(border: Border.all(color: bloodBright, width: 0.8)),
            child: Text('ON AIR  ${fmtTime((live['since_ms'] as num).toInt())}', style: mono(13, color: bloodBright, weight: FontWeight.w700)),
          ),
          const SizedBox(width: 12),
          button('STOP', () async {
            await Native.callAway({'cmd': 'end_live'});
            _service.invokeMethod('live', {'on': false}).catchError((_) {});
          }),
        ])
      else
        Row(children: [
          button('GO LIVE', () async {
            if (!await askMic(context)) return;
            final r = await Native.callAway({'cmd': 'go_live', 'station': id});
            if (!mounted) return;
            if (r is Map && r['error'] != null) {
              setState(() => error = r['error']);
            } else {
              _service.invokeMethod('live', {'on': true}).catchError((_) {});
            }
          }, color: bloodBright),
        ]),
      if (error != null) Padding(padding: const EdgeInsets.only(top: 10), child: Text(error!, style: serif(14, color: bone, italic: true))),
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
      if (mounted) setState(() => programmeFor = null);
    }

    return [
      heading('WHEN IT PLAYS'),
      note(programmeSummary(programme)),
      const SizedBox(height: 12),
      Wrap(spacing: 6, runSpacing: 6, children: [
        button('CHANGE THE SCHEDULE', () => after(Navigator.push(context, MaterialPageRoute(builder: (_) => Scaffold(backgroundColor: black, appBar: AppBar(backgroundColor: black), body: ProgrammeEditor(station: id, name: (mine['name'] ?? '').toString())))))),
        button('PLAN A PREMIERE', () => after(pyrateSheet(context, ShowEditor(station: id)))),
        button('POST A NOTICE', () => after(pyrateSheet(context, NoticeEditor(station: id)))),
        button('SEE YOUR PAGE', () => openPage(context, mine, widget.radio)),
        if (widget.radio.stations.any((x) => x['id'] == id && x['pocket'] == true))
          button('INVITE LISTENERS', () => pyrateSheet(context, InviteSheet(station: id)))
        else
          button('MAKE A POSTER', () => pyrateSheet(context, PosterSheet(station: mine))),
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
            Text(mine['name'] ?? '', style: pica(28, spacing: 0.03)),
            Text(mine['call_sign'] ?? '', style: mono(12)),
            const SizedBox(height: 6),
            Text('${mine['tracks']} ${mine['tracks'] == 1 ? 'recording' : 'recordings'}', style: serif(14, color: boneFaint, italic: true)),
            if (mine['waiting'] == true)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Text('Waiting for another radio to pass your station on, so nobody can trace it to this phone. It goes out as soon as one does.',
                    style: serif(13, color: boneDim, italic: true)),
              ),
            if (mine['direct'] == true)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Text('Few radios are online, so your station is going out straight from this phone. Listeners can see this phone\'s address until more radios join.',
                    style: serif(13, color: boneDim, italic: true)),
              ),
          ]),
        ),
      ]),
      AboardChart(station: id),
      const SizedBox(height: 30),
      heading('ADD RECORDINGS'),
      note('Music, shows or readings from your files. Name a file "Artist - Title" and both show.'),
      setting('Spoken word', talk, (v) => setState(() => talk = v), sub: 'For talk and readings. Smaller files, so they reach listeners sooner. Leave it off for music.'),
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
      if (library.isNotEmpty) ...[
        const SizedBox(height: 22),
        Text('YOUR RECORDINGS', style: carved(12, color: boneFaint, spacing: 0.3)),
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
                tooltip: 'Remove',
                onPressed: () {
                  Native.call({'cmd': 'withdraw', 'station': id, 'track': t['hash']});
                  libraryCount = -1;
                },
              ),
            ]),
          ),
      ],
      const SizedBox(height: 30),
      ..._programme(id, mine),
      const SizedBox(height: 30),
      ..._onAir(id),
      const SizedBox(height: 30),
      RecordPanel(station: id),
      const SizedBox(height: 30),
      ..._art(id, mine),
      const SizedBox(height: 30),
      DetailsPanel(station: id),
      const SizedBox(height: 30),
      fold('More', sub: 'Podcast feeds, co-hosts, numbers, ending the station', [
        FeedsPanel(station: id),
        const SizedBox(height: 26),
        HandsPanel(station: id),
        const SizedBox(height: 26),
        heading('A NUMBERS STATION'),
        note('Five-figure groups read out by the phone, like the shortwave numbers stations. They play like any other recording.'),
        const SizedBox(height: 10),
        Row(children: [button('SET ONE UP', () => pyrateSheet(context, NumbersKit(station: id)))]),
        const SizedBox(height: 26),
        heading('YOUR 24 WORDS'),
        note('The 24 words you wrote down are the station itself. With them, any phone can take the station back from this one, or end it. This phone can broadcast but cannot end the station.'),
        const SizedBox(height: 26),
        heading('END THE STATION'),
        note('Takes it off the air for good. You need your 24 words.'),
        const SizedBox(height: 10),
        Row(children: [
          button('END THE STATION', () async {
      final words = TextEditingController();
      final wpass = TextEditingController();
      await secure(true);
      if (!context.mounted) return;
      final ok = await showDialog<bool>(
        context: context,
        builder: (c) => AlertDialog(
          backgroundColor: panel,
          shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
          title: Text('End the station?', style: serif(22)),
          content: Column(mainAxisSize: MainAxisSize.min, children: [
            Text('It goes off the air for good, everywhere. This cannot be undone.', style: serif(15, color: boneFaint)),
            TextField(controller: words, style: mono(12, color: bone), maxLines: 3, decoration: field('The 24 words'),
                enableSuggestions: false, autocorrect: false, enableIMEPersonalizedLearning: false, keyboardType: TextInputType.visiblePassword),
            TextField(controller: wpass, obscureText: true, style: mono(12, color: bone), decoration: field('Passphrase, if you set one'),
            enableSuggestions: false, autocorrect: false, enableIMEPersonalizedLearning: false),
          ]),
          actions: [
            TextButton(onPressed: () => Navigator.pop(c, false), child: Text('KEEP IT', style: carved(11))),
            TextButton(onPressed: () => Navigator.pop(c, true), child: Text('END IT', style: carved(11))),
          ],
        ),
      );
      await secure(false);
      final phrase = words.text, pp = wpass.text;
      words.clear();
      words.dispose();
      wpass.dispose();
      if (ok == true) {
        final r = await Native.callAway({'cmd': 'strike', 'station': id, 'phrase': phrase, 'passphrase': pp});
        if (!context.mounted) return;
        toast(context, r is Map && r['error'] != null ? 'Not ended: ${r['error']}' : 'The station has ended.');
      }
          }, color: bloodBright),
        ]),
      ]),
    ];
  }
}


/// An old phone in a drawer, on a charger and Wi-Fi, keeping a share of the fleet afloat
/// all day. It gives what its owner chooses, fills only while it charges on Wi-Fi, and goes
/// quiet when switched off.
class DrawerLantern extends StatefulWidget {
  const DrawerLantern({super.key});
  @override
  State<DrawerLantern> createState() => _DrawerLanternState();
}

class _DrawerLanternState extends State<DrawerLantern> {
  bool on = pref('lantern') == true;
  int gb = (pref('lantern_gb') as num?)?.toInt() ?? 4;

  void apply() {
    pref('lantern', on);
    pref('lantern_gb', gb);
    Native.call({'cmd': 'ballast_size', 'gb': on ? gb : 0});
    _service.invokeMethod('lantern', {'on': on, 'gb': gb}).catchError((_) {});
  }

  Widget sizeChip(int n) => Padding(
        padding: const EdgeInsets.only(right: 8, bottom: 8),
        child: InkWell(
          onTap: () => setState(() {
            gb = n;
            apply();
          }),
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
            decoration: BoxDecoration(border: Border.all(color: gb == n ? bone : boneGhost, width: gb == n ? 0.9 : 0.5)),
            child: Text('$n GB', style: carved(9.5, color: gb == n ? bone : boneFaint, spacing: 0.2)),
          ),
        ),
      );

  @override
  Widget build(BuildContext context) => Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        setting('Use this as a spare phone', on, (v) => setState(() {
              on = v;
              apply();
            }), sub: 'Got an old phone in a drawer? Leave it on a charger and Wi-Fi, and it helps carry stations all day with the screen off. Choose how much space to give.'),
        if (on) Wrap(children: [for (final n in [4, 16, 64, 128]) sizeChip(n)]),
      ]);
}
