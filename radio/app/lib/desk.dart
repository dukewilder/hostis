// Pyrate Radio: the live desk. The microphone, a deck of music, six pads and, on a computer, a
// second input, mixed in the node (desk.rs) and sent as the show is made. The desk asks the
// node how it stands ten times a second while anything sounds, and rests otherwise.
// No rights reserved.

import 'dart:async';
import 'dart:io';
import 'dart:math' as math;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';

import 'main.dart';
import 'more.dart';
import 'native.dart';
import 'shell.dart';
import 'station.dart' show chip;

const _shell = ShellChannel();

/// The desk's own ground, a shade off black, and the deep red of a key held down.
const deskInk = Color(0xFF0B0A08);
const bloodDeep = Color(0xFF5A1010);
const _bloodHeld = Color(0xFF6E1414);

const _liveNote = 'Listeners hear you about fifteen seconds behind. Sounds in the background can give away where you are.';

// A fader stands level at 80, ten decibels hotter at the top and off at the bottom, as the browser's.
double _fdb(double v) => (v - 80) / 2;
double _gain(double v) => v <= 0 ? 0 : math.pow(10, _fdb(v) / 20).toDouble();
double _faderAt(num g) => g <= 0 ? 0 : (80 + 40 * math.log(g) / math.ln10).clamp(0, 100).toDouble();
String _fsay(double v) {
  if (v <= 0) return 'Off';
  final d = _fdb(v).round();
  return d == 0 ? '0 dB' : '${d > 0 ? '+' : '−'}${d.abs()} dB';
}

String _clock(int ms) {
  final s = ms ~/ 1000;
  String two(int x) => x.toString().padLeft(2, '0');
  return s >= 3600 ? '${s ~/ 3600}:${two(s ~/ 60 % 60)}:${two(s % 60)}' : '${two(s ~/ 60)}:${two(s % 60)}';
}

/// "Artist - Title.mp3" names both, as recordings do.
Map<String, dynamic> _named(String path, String name) {
  final stem = name.replaceAll(RegExp(r'\.[^.]+$'), '');
  final parts = stem.split(' - ');
  return {'path': path, 'title': parts.length > 1 ? parts.sublist(1).join(' - ') : stem, 'by': parts.length > 1 ? parts.first : ''};
}

double _num(dynamic v, [double or = 0]) => v is num ? v.toDouble() : or;

class LiveDesk extends StatefulWidget {
  final String station;
  const LiveDesk({super.key, required this.station});
  @override
  State<LiveDesk> createState() => _LiveDeskState();
}

class _LiveDeskState extends State<LiveDesk> with WidgetsBindingObserver {
  Map<String, dynamic> d = const {};
  List list = const [];
  int listV = -1;
  Timer? _timer;
  bool starting = false;
  bool wasLive = false;
  int? loadingPad;
  String? said;
  List<String> inputs = const [];
  // Where a fader is while a finger is on it, so the next reading never snaps it back.
  final Map<String, double> held = {};
  static bool _previewed = false;

  bool get computer => !Platform.isAndroid && !Platform.isIOS;
  bool get live => d['station'] == widget.station;
  bool get checking => d['checking'] == true;
  List get pads => (d['pads'] as List?) ?? List.filled(6, null);
  bool get sounding => live || checking || d['hear'] == true || (d['deck'] as Map?)?['playing'] == true || pads.any((p) => p is Map && p['playing'] == true);

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _preview();
    _read();
    if (computer) {
      Native.callAway({'cmd': 'desk_devices'}).then((r) {
        if (mounted && r is Map) setState(() => inputs = [for (final x in (r['inputs'] as List? ?? [])) x.toString()]);
      });
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  // With the screen off the show goes on in the node; only the desk's drawing rests.
  @override
  void didChangeAppLifecycleState(AppLifecycleState s) {
    if (s == AppLifecycleState.resumed) {
      _read();
    } else if (s == AppLifecycleState.paused) {
      _timer?.cancel();
      _timer = null;
    }
  }

  void _take(Map<String, dynamic> r) {
    final v = ((r['deck'] as Map?)?['v'] as num?)?.toInt() ?? 0;
    if (v != listV) {
      final full = Native.call({'cmd': 'desk', 'list': true});
      if (full is Map) {
        list = (full['list'] as List?) ?? const [];
        listV = v;
      }
    }
    final on = r['station'] == widget.station;
    // A show that ended on its own lets the phone's foreground service go.
    if (wasLive && !on && !starting) _shell.invokeMethod('live', {'on': false}).catchError((_) {});
    wasLive = on;
    d = r;
  }

  void _read() {
    _timer?.cancel();
    final r = Native.call({'cmd': 'desk', 'list': false});
    if (r is Map<String, dynamic> && r['error'] == null && mounted) setState(() => _take(r));
    if (!mounted) return;
    _timer = Timer(Duration(milliseconds: sounding ? 100 : 700), _read);
  }

  /// A change to the desk; the node answers with the desk as it now stands.
  void _set(Map<String, dynamic> req) {
    final r = Native.call(req);
    if (r is! Map<String, dynamic>) return;
    setState(() {
      if (r['error'] != null) {
        said = r['error'].toString();
      } else {
        said = null;
        _take(r);
      }
    });
    if (sounding) _read();
  }

  String _plain(String e) => e.startsWith('no microphone') ? 'No microphone would open. Plug one in or allow it, then try again.' : e;

  bool get _music => list.isNotEmpty || pads.any((p) => p != null) || (d['two'] as Map?)?['on'] == true;

  Future<void> _goLive() async {
    if (starting) return;
    if (live) {
      await Native.callAway({'cmd': 'end_live'});
      _shell.invokeMethod('live', {'on': false}).catchError((_) {});
      wasLive = false;
      _read();
      return;
    }
    setState(() {
      starting = true;
      said = null;
    });
    // A show of music alone can go out without a microphone; a show of nothing cannot.
    final mic = await askMic(context);
    if (!mounted) return;
    if (!mic && !_music) {
      setState(() => starting = false);
      return;
    }
    final r = await Native.callAway({'cmd': 'go_live', 'station': widget.station});
    if (!mounted) return;
    if (r is Map && r['error'] != null) {
      setState(() {
        said = _plain(r['error'].toString());
        starting = false;
      });
      return;
    }
    // The phone keeps the show going with the screen off, as it keeps the radio playing.
    _shell.invokeMethod('live', {'on': true}).catchError((_) {});
    wasLive = true;
    setState(() {
      starting = false;
      said = r is Map ? r['note'] as String? : null;
    });
    _read();
  }

  Future<void> _check() async {
    if (checking) return _set({'cmd': 'desk_set', 'check': false});
    if (!await askMic(context)) return;
    _set({'cmd': 'desk_set', 'check': true});
  }

  void _disguise() {
    final on = d['disguise'] == true;
    pref('disguise', on ? 0 : -4);
    _set({'cmd': 'desk_set', 'disguise': on ? 0 : -4});
  }

  Future<void> _addMusic() async {
    final picked = await FilePicker.platform.pickFiles(type: FileType.audio, allowMultiple: true);
    final files = [for (final f in picked?.files ?? <PlatformFile>[]) if (f.path != null) _named(f.path!, f.name)];
    if (files.isEmpty || !mounted) return;
    _set({'cmd': 'deck_add', 'files': files});
  }

  void _drop(int i) {
    final r = Native.call({'cmd': 'deck_remove', 'at': i});
    // The picker's copy of the file goes with it; a file kept elsewhere stays where it is.
    if (r is Map && r['path'] is String) unawaited(dropCopy(r['path'] as String));
    _read();
  }

  Future<void> _pad(int i) async {
    if (pads[i] != null) return _set({'cmd': 'pad_fire', 'slot': i});
    final picked = await FilePicker.platform.pickFiles(type: FileType.audio);
    final f = picked?.files.single;
    if (f?.path == null || !mounted) return;
    setState(() => loadingPad = i);
    final r = await Native.callAway({'cmd': 'pad_load', 'slot': i, 'path': f!.path, 'name': _named(f.path!, f.name)['title']});
    // A pad's sound is held in memory once loaded, so the picker's copy can go at once.
    unawaited(dropCopy(f.path));
    if (!mounted) return;
    setState(() {
      loadingPad = null;
      if (r is Map && r['error'] != null) {
        said = r['error'].toString();
      } else if (r is Map<String, dynamic>) {
        _take(r);
      }
    });
  }

  /// For looking at the desk on a computer with no microphone (PYRATE_PREVIEW_DESK=<folder>):
  /// the folder's sounds go on the deck and the pads, and mic.wav stands in for the
  /// microphone. A phone has no such setting.
  void _preview() {
    final dir = computer ? Platform.environment['PYRATE_PREVIEW_DESK'] : null;
    if (_previewed || dir == null || !Directory(dir).existsSync()) return;
    _previewed = true;
    final files = Directory(dir).listSync().whereType<File>().map((f) => f.path).where((p) => RegExp(r'\.(wav|mp3|flac|ogg|opus|m4a)$', caseSensitive: false).hasMatch(p)).toList()..sort();
    String name(String p) => p.split(Platform.pathSeparator).last;
    for (final p in files) {
      if (name(p) == 'mic.wav') Native.call({'cmd': 'desk_set', 'mic_file': p});
      if (name(p) == 'two.wav') Native.call({'cmd': 'desk_set', 'two_file': p});
    }
    final pads = files.where((p) => name(p).startsWith('pad')).toList();
    final music = files.where((p) => !name(p).startsWith('pad') && name(p) != 'mic.wav' && name(p) != 'two.wav').toList();
    if (music.isNotEmpty) Native.call({'cmd': 'deck_add', 'files': [for (final p in music) _named(p, name(p))]});
    for (var i = 0; i < pads.length && i < 6; i++) {
      Native.call({'cmd': 'pad_load', 'slot': i, 'path': pads[i], 'name': _named(pads[i], name(pads[i]).replaceFirst(RegExp(r'^pad\d* *-? *'), ''))['title']});
    }
  }

  // ---- The parts -----------------------------------------------------------------------

  static const _rule = Divider(height: 1, thickness: 1, color: line);

  Widget _head(String label, {Widget? trailing}) => ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 40),
        child: Row(children: [
          Expanded(child: Text(label, maxLines: 1, overflow: TextOverflow.ellipsis, style: caps(11, spacing: 0.26))),
          ?trailing,
        ]),
      );

  Widget _mini(String label, VoidCallback? onTap) => OutlinedButton(
        onPressed: onTap,
        style: OutlinedButton.styleFrom(
          minimumSize: const Size(0, 36),
          padding: const EdgeInsets.symmetric(horizontal: 12),
          side: BorderSide(color: onTap == null ? lineSoft : boneGhost, width: 1),
          shape: const RoundedRectangleBorder(),
        ),
        child: Text(label, style: carved(10.5, color: onTap == null ? boneGhost : boneDim, spacing: 0.2)),
      );

  /// A key that stays down: red while on the air, bone while only checking.
  Widget _key(String label, bool on, VoidCallback onTap, {double height = 52}) {
    final lit = on && live;
    return Semantics(
      button: true,
      toggled: on,
      label: label,
      child: InkWell(
        onTap: onTap,
        child: Container(
          height: height,
          padding: const EdgeInsets.symmetric(horizontal: 12),
          decoration: BoxDecoration(color: lit ? bloodDeep : Colors.transparent, border: Border.all(color: lit ? red : on ? boneDim : boneGhost, width: 1)),
          child: ExcludeSemantics(
            child: Row(mainAxisAlignment: MainAxisAlignment.center, children: [
              Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: lit ? bloodBright : on ? bone : boneGhost)),
              const SizedBox(width: 10),
              Flexible(child: Text(label, maxLines: 1, overflow: TextOverflow.ellipsis, style: carved(12, color: on ? bone : boneFaint, spacing: 0.22))),
            ]),
          ),
        ),
      ),
    );
  }

  Widget _fader(String key, dynamic gain, String label) {
    final v = held[key] ?? _fader0(gain);
    return Row(children: [
      Expanded(
        child: SliderTheme(
          data: _faderTheme,
          child: Slider(
            value: v,
            max: 100,
            semanticFormatterCallback: (x) => '$label ${_fsay(x)}',
            onChanged: (x) {
              setState(() => held[key] = x);
              Native.call({'cmd': 'desk_set', key: _gain(x)});
            },
            onChangeEnd: (_) => held.remove(key),
          ),
        ),
      ),
      SizedBox(width: 54, child: Text(_fsay(v), textAlign: TextAlign.right, style: mono(11, color: boneDim))),
    ]);
  }

  double _fader0(dynamic g) => _faderAt(_num(g, 1));

  Widget _air() {
    final since = (d['since_ms'] as num?)?.toInt() ?? 0;
    final air = (d['air'] as List?) ?? const [0, 0];
    final note = said ?? d['note'] as String? ?? (live ? null : _liveNote);
    return Padding(
      padding: const EdgeInsets.all(14),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Semantics(
          button: true,
          label: live ? 'End the broadcast' : 'Go live',
          child: InkWell(
            onTap: starting ? null : _goLive,
            child: Container(
              height: 54,
              decoration: BoxDecoration(color: live ? Colors.transparent : bloodDeep, border: Border.all(color: live ? bone : red, width: 1)),
              child: ExcludeSemantics(
                child: Row(mainAxisAlignment: MainAxisAlignment.center, children: [
                  Text(live ? '■︎' : '●︎', style: TextStyle(fontFamily: 'Mono', fontSize: 13, height: 1, color: live ? bone : bloodBright)),
                  const SizedBox(width: 12),
                  Text(starting ? 'GOING LIVE' : live ? 'END THE BROADCAST' : 'GO LIVE', style: carved(13, spacing: 0.22)),
                ]),
              ),
            ),
          ),
        ),
        const SizedBox(height: 12),
        // The lamp stays beside the first words however narrow the desk.
        Row(children: [
          Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: live ? bloodBright : boneGhost)),
          const SizedBox(width: 10),
          Expanded(
            child: Wrap(crossAxisAlignment: WrapCrossAlignment.center, spacing: 12, runSpacing: 4, children: [
              if (live)
                Text('ON AIR  ${_clock(since)}', style: mono(13, color: bloodBright, weight: FontWeight.w700))
              else
                Text(checking ? 'OFF THE AIR · CHECKING LEVELS' : 'OFF THE AIR', style: caps(checking ? 10 : 11, spacing: checking ? 0.18 : 0.24)),
              if (live && d['aboard'] is num) Text('${d['aboard']} LISTENING', style: caps(11, color: boneDim, spacing: 0.2)),
            ]),
          ),
        ]),
        if (note != null) Padding(padding: const EdgeInsets.only(top: 8), child: Text(note, style: serif(14, color: said != null ? bone : boneFaint, italic: true))),
        const SizedBox(height: 12),
        for (final (i, side) in [(0, 'L'), (1, 'R')])
          Padding(
            padding: EdgeInsets.only(top: i == 0 ? 0 : 5),
            child: Row(children: [
              SizedBox(width: 14, child: Text(side, style: mono(10))),
              Expanded(child: Meter(level: _num(air.length > i ? air[i] : 0), height: 8)),
            ]),
          ),
        if (!live) ...[
          const SizedBox(height: 12),
          Row(children: [_mini(checking ? 'STOP CHECKING' : 'CHECK LEVELS', starting ? null : _check)]),
        ],
      ]),
    );
  }

  Widget _mic() {
    final m = (d['mic'] as Map?) ?? const {};
    final open = m['open'] != false;
    final on = m['on'] == true;
    final err = m['err'] as String?;
    // The microphone's own choice, on a computer with more than one.
    final dev = computer && inputs.length > 1
        ? DropdownButtonHideUnderline(
            child: DropdownButton<String>(
              value: inputs.contains(m['dev']) ? m['dev'] as String : inputs.first,
              isDense: true,
              dropdownColor: panel,
              iconEnabledColor: boneFaint,
              style: mono(11, color: boneDim),
              items: [for (final n in inputs) DropdownMenuItem(value: n, child: ConstrainedBox(constraints: const BoxConstraints(maxWidth: 170), child: Text(n, overflow: TextOverflow.ellipsis)))],
              onChanged: (v) => _set({'cmd': 'desk_set', 'mic_dev': v == inputs.first ? '' : v}),
            ),
          )
        : null;
    return Container(
      // A red rule along the top while the open microphone is on the air.
      decoration: BoxDecoration(border: Border(top: BorderSide(color: live && open && on ? red : Colors.transparent, width: 2))),
      padding: const EdgeInsets.fromLTRB(14, 6, 14, 14),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        _head('MICROPHONE', trailing: dev),
        Meter(level: on ? _num(m['level']) : 0),
        _fader('mic_gain', m['gain'], 'Microphone level'),
        Row(children: [
          Expanded(child: _key(open ? 'MIC OPEN' : 'MIC CLOSED', open, () => _set({'cmd': 'desk_set', 'mic_open': !open}))),
          const SizedBox(width: 8),
          Semantics(
            button: true,
            toggled: d['disguise'] == true,
            label: 'Disguise my voice',
            child: InkWell(
              onTap: _disguise,
              child: Container(
                height: 52,
                padding: const EdgeInsets.symmetric(horizontal: 14),
                alignment: Alignment.center,
                decoration: BoxDecoration(border: Border.all(color: d['disguise'] == true ? boneDim : boneGhost, width: 1)),
                child: ExcludeSemantics(child: Text('DISGUISE', style: carved(10.5, color: d['disguise'] == true ? bone : boneFaint, spacing: 0.2))),
              ),
            ),
          ),
        ]),
        if (err != null && !on && (live || checking)) Padding(padding: const EdgeInsets.only(top: 8), child: Text(_plain(err.startsWith('no microphone') ? err : 'no microphone: $err'), style: serif(13, color: boneDim, italic: true))),
      ]),
    );
  }

  Widget _two() {
    final t = (d['two'] as Map?) ?? const {};
    final open = t['open'] != false;
    return Padding(
      padding: const EdgeInsets.fromLTRB(14, 6, 14, 14),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        _head(((t['dev'] as String?) ?? 'Second input').toUpperCase(), trailing: _mini('OFF', () => _set({'cmd': 'desk_set', 'two_dev': null, 'two_file': null}))),
        Meter(level: _num(t['level'])),
        _fader('two_gain', t['gain'], 'Second input level'),
        _key(open ? 'OPEN' : 'CLOSED', open, () => _set({'cmd': 'desk_set', 'two_open': !open})),
      ]),
    );
  }

  Widget _deck() {
    final k = (d['deck'] as Map?) ?? const {};
    final at = (k['at'] as num?)?.toInt();
    final playing = k['playing'] == true;
    final pos = (k['pos_ms'] as num?)?.toInt() ?? 0;
    final len = (k['len_ms'] as num?)?.toInt();
    final title = k['title'] as String?;
    final by = (k['by'] as String?) ?? '';
    Widget tb(IconData icon, String label, VoidCallback onTap, {bool lead = false}) => Semantics(
          button: true,
          label: label,
          child: InkWell(
            onTap: onTap,
            child: Container(
              width: lead ? 58 : 48,
              height: 48,
              decoration: BoxDecoration(color: lead ? bloodDeep : Colors.transparent, border: Border.all(color: lead ? red : boneGhost, width: 1)),
              child: Icon(icon, size: 22, color: lead ? bone : boneDim),
            ),
          ),
        );
    return Padding(
      padding: const EdgeInsets.fromLTRB(14, 6, 14, 14),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        _head('DECK', trailing: _mini('ADD MUSIC', _addMusic)),
        Row(crossAxisAlignment: CrossAxisAlignment.end, children: [
          Expanded(
            child: Text(
              title == null ? (list.isEmpty ? 'Empty' : 'Nothing playing') : by.isEmpty ? title : '$title · $by',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: serif(16, color: title == null ? boneFaint : bone, italic: title == null),
            ),
          ),
          if (title != null) Text(len != null ? '${fmtTime(pos)}  −${fmtTime(math.max(0, len - pos))}' : fmtTime(pos), style: mono(11, color: boneDim)),
        ]),
        const SizedBox(height: 8),
        // The track's progress: a hairline, the red diamond where it has reached.
        SizedBox(
          height: 12,
          child: CustomPaint(painter: _Progress(title != null && len != null && len > 0 ? (pos / len).clamp(0.0, 1.0) : null)),
        ),
        const SizedBox(height: 8),
        Row(children: [
          tb(playing ? Icons.pause : Icons.play_arrow, playing ? 'Pause' : 'Play', () => _set({'cmd': playing ? 'deck_pause' : 'deck_play'}), lead: true),
          const SizedBox(width: 8),
          tb(Icons.skip_next, 'Skip', () => _set({'cmd': 'deck_next'})),
          const SizedBox(width: 12),
          Expanded(child: Meter(level: _num(k['level']), height: 8)),
        ]),
        _fader('deck_gain', k['gain'], 'Deck level'),
        if (list.isEmpty)
          InkWell(
            onTap: _addMusic,
            child: Container(
              height: 64,
              alignment: Alignment.center,
              decoration: BoxDecoration(border: Border.all(color: boneGhost, width: 1)),
              child: Text('Choose music for the deck', style: serif(15, color: boneFaint, italic: true)),
            ),
          )
        else
          ConstrainedBox(
            // A long set scrolls in its own space; the desk keeps its shape.
            constraints: const BoxConstraints(maxHeight: 6 * 45),
            child: ListView.builder(
              shrinkWrap: true,
              padding: EdgeInsets.zero,
              itemCount: list.length,
              itemBuilder: (c, i) => _row(i, (list[i] as Map?) ?? const {}, i == at),
            ),
          ),
      ]),
    );
  }

  Widget _row(int i, Map x, bool on) {
    final bad = x['bad'] == true;
    final by = (x['by'] as String?) ?? '';
    final len = (x['len_ms'] as num?)?.toInt();
    return InkWell(
      onTap: bad ? null : () => _set({'cmd': 'deck_play', 'at': i}),
      child: Container(
        constraints: const BoxConstraints(minHeight: 44),
        decoration: BoxDecoration(border: Border(left: BorderSide(color: on ? red : Colors.transparent, width: 2), bottom: const BorderSide(color: lineSoft, width: 1))),
        child: Row(children: [
          SizedBox(width: 26, child: Text('${i + 1}', textAlign: TextAlign.right, style: mono(10, color: on ? bloodBright : boneFaint))),
          const SizedBox(width: 10),
          Expanded(
            child: Text.rich(
              TextSpan(text: (x['title'] as String?) ?? '', children: [if (by.isNotEmpty) TextSpan(text: '  $by', style: serif(13, color: boneFaint, italic: true))]),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: serif(15, color: bad ? boneFaint : on ? bone : boneDim),
            ),
          ),
          Text(bad ? 'Will not play' : len != null ? fmtTime(len) : '', style: mono(11, color: boneFaint)),
          IconButton(icon: const Icon(Icons.close, size: 16, color: boneFaint), tooltip: 'Remove', visualDensity: VisualDensity.compact, onPressed: () => _drop(i)),
        ]),
      ),
    );
  }

  Widget _pads(bool wide) {
    final cols = wide ? 6 : 3;
    return Padding(
      padding: const EdgeInsets.fromLTRB(14, 6, 14, 14),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        _head('PADS', trailing: Text('HOLD ONE TO CLEAR IT', style: caps(9.5, color: boneGhost, spacing: 0.16))),
        LayoutBuilder(
          builder: (c, b) {
            final w = (b.maxWidth - 8 * (cols - 1)) / cols;
            return Wrap(spacing: 8, runSpacing: 8, children: [for (var i = 0; i < 6; i++) SizedBox(width: w, height: 64, child: _pad1(i, pads.length > i && pads[i] is Map ? pads[i] as Map : null))]);
          },
        ),
        const SizedBox(height: 6),
        _fader('pads_gain', d['pads_gain'], 'Pads level'),
        Meter(level: _num(d['pads_level'])),
      ]),
    );
  }

  Widget _pad1(int i, Map? p) {
    final playing = p?['playing'] == true;
    final name = loadingPad == i ? 'LOADING' : p == null ? 'LOAD' : (p['name'] as String? ?? '').toUpperCase();
    return Semantics(
      button: true,
      label: p == null ? 'Load pad ${i + 1}' : 'Pad ${i + 1}, ${p['name']}',
      child: InkWell(
        onTap: loadingPad == null ? () => _pad(i) : null,
        onLongPress: p == null ? null : () => _set({'cmd': 'pad_clear', 'slot': i}),
        child: Container(
          decoration: BoxDecoration(color: p == null ? Colors.transparent : playing ? _bloodHeld : bloodDeep, border: Border.all(color: p == null ? boneGhost : red, width: 1)),
          child: Stack(children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(9, 7, 9, 9),
              child: ExcludeSemantics(
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
                  Text('${i + 1}', style: mono(10, color: p == null ? boneFaint : boneDim)),
                  Text(name, maxLines: 2, overflow: TextOverflow.ellipsis, style: caps(10, color: p == null ? boneFaint : bone, spacing: 0.12).copyWith(height: 1.25)),
                ]),
              ),
            ),
            if (playing)
              Positioned(
                left: 0,
                right: 0,
                bottom: 0,
                child: Align(alignment: Alignment.centerLeft, child: FractionallySizedBox(widthFactor: _num(p?['pos']).clamp(0.0, 1.0), child: Container(height: 2, color: bloodBright))),
              ),
          ]),
        ),
      ),
    );
  }

  Widget _more() {
    final duck = _num(d['duck'], 0.25);
    // Any input but the one the microphone is on, which is the first, the default, unless chosen.
    final mic = ((d['mic'] as Map?)?['dev'] as String?) ?? (inputs.isEmpty ? null : inputs.first);
    final others = [for (final n in inputs) if (n != mic) n];
    final two = (d['two'] as Map?)?['dev'] as String?;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 14),
      child: fold('More', [
        Text('MUSIC UNDER THE MIC', style: caps(10.5, spacing: 0.22)),
        const SizedBox(height: 8),
        Wrap(children: [
          for (final (v, l) in [(1.0, 'Stays up'), (0.5, 'Half'), (0.25, 'A quarter'), (0.125, 'An eighth')]) chip(l, (duck - v).abs() < 0.01, () => _set({'cmd': 'desk_set', 'duck': v})),
        ]),
        setting('Stop after each track', d['each'] == true, (v) => _set({'cmd': 'desk_set', 'each': v})),
        if (computer && others.isNotEmpty) ...[
          const SizedBox(height: 6),
          Text('SECOND INPUT', style: caps(10.5, spacing: 0.22)),
          const SizedBox(height: 4),
          note('A guest\'s microphone or an interface. It loses its hum and nothing else.'),
          const SizedBox(height: 8),
          Wrap(children: [
            chip('Off', two == null, () => _set({'cmd': 'desk_set', 'two_dev': null})),
            for (final n in others) chip(n.length > 28 ? '${n.substring(0, 27)}…' : n, two == n, () => _set({'cmd': 'desk_set', 'two_dev': n})),
          ]),
        ],
      ], sub: others.isNotEmpty ? 'Music under the mic, track by track, a second input' : 'Music under the mic, track by track'),
    );
  }

  @override
  Widget build(BuildContext context) {
    final t = (d['two'] as Map?) ?? const {};
    return LayoutBuilder(
      builder: (context, box) => Container(
        decoration: BoxDecoration(color: deskInk, border: Border.all(color: live ? red : boneGhost, width: 1)),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Padding(
            padding: const EdgeInsets.only(left: 14, right: 4),
            child: Row(children: [
              Expanded(child: Text('LIVE DESK', maxLines: 1, softWrap: false, overflow: TextOverflow.fade, style: caps(10.5, spacing: 0.22))),
              Semantics(
                toggled: d['hear'] == true,
                label: 'Hear the mix, on headphones',
                child: InkWell(
                  onTap: () => _set({'cmd': 'desk_set', 'hear': d['hear'] != true}),
                  child: Container(
                    constraints: const BoxConstraints(minHeight: 44),
                    padding: const EdgeInsets.symmetric(horizontal: 8),
                    child: ExcludeSemantics(
                      child: Row(mainAxisSize: MainAxisSize.min, children: [
                        Container(
                          width: 13,
                          height: 13,
                          decoration: BoxDecoration(color: d['hear'] == true ? bone : Colors.transparent, border: Border.all(color: d['hear'] == true ? bone : boneFaint, width: 1)),
                          child: d['hear'] == true ? const Icon(Icons.check, size: 11, color: black) : null,
                        ),
                        const SizedBox(width: 8),
                        Text('HEAR THE MIX', style: caps(10, color: d['hear'] == true ? bone : boneFaint, spacing: 0.14)),
                        const SizedBox(width: 6),
                        Text('headphones', style: mono(9.5, color: boneFaint)),
                      ]),
                    ),
                  ),
                ),
              ),
            ]),
          ),
          _rule,
          _air(),
          _rule,
          _mic(),
          if (t['on'] == true || t['dev'] != null) ...[_rule, _two()],
          _rule,
          _deck(),
          _rule,
          _pads(box.maxWidth >= 480),
          _rule,
          _more(),
        ]),
      ),
    );
  }
}

final _faderTheme = SliderThemeData(
  trackHeight: 2,
  activeTrackColor: boneFaint,
  inactiveTrackColor: boneGhost,
  thumbColor: bone,
  overlayShape: SliderComponentShape.noOverlay,
  thumbShape: const _Knob(),
  trackShape: const RectangularSliderTrackShape(),
);

/// A fader's knob, as the browser's: a bone block with a black line through it.
class _Knob extends SliderComponentShape {
  const _Knob();
  @override
  Size getPreferredSize(bool isEnabled, bool isDiscrete) => const Size(12, 26);
  @override
  void paint(PaintingContext context, Offset center,
      {required Animation<double> activationAnimation,
      required Animation<double> enableAnimation,
      required bool isDiscrete,
      required TextPainter labelPainter,
      required RenderBox parentBox,
      required SliderThemeData sliderTheme,
      required TextDirection textDirection,
      required double value,
      required double textScaleFactor,
      required Size sizeWithOverflow}) {
    context.canvas.drawRect(Rect.fromCenter(center: center, width: 12, height: 26), Paint()..color = bone);
    context.canvas.drawRect(Rect.fromCenter(center: center, width: 2, height: 26), Paint()..color = black);
  }
}

/// A level: lit segments up the scale from 48 dB down to full, the top tenth in red.
class Meter extends StatelessWidget {
  final double level;
  final double height;
  const Meter({super.key, required this.level, this.height = 6});
  @override
  Widget build(BuildContext context) => SizedBox(height: height, child: CustomPaint(painter: _MeterPaint(level), size: Size.infinite));
}

class _MeterPaint extends CustomPainter {
  final double level;
  _MeterPaint(this.level);
  @override
  void paint(Canvas canvas, Size size) {
    final n = (size.width / 7).floor().clamp(12, 40);
    final db = level > 1e-4 ? 20 * math.log(level) / math.ln10 : -99.0;
    final lit = ((db + 48) / 48 * n).round().clamp(0, n);
    const gap = 1.0;
    final w = (size.width - gap * (n - 1)) / n;
    for (var i = 0; i < n; i++) {
      final c = i >= lit ? const Color(0x17EDE4CE) : i >= n * 0.9 ? bloodBright : i >= n * 0.7 ? bone : const Color(0x8CEDE4CE);
      canvas.drawRect(Rect.fromLTWH(i * (w + gap), 0, w, size.height), Paint()..color = c);
    }
  }

  @override
  bool shouldRepaint(_MeterPaint old) => old.level != level;
}

/// A track's way through: a hairline, its played part in bone, a red diamond at the head.
class _Progress extends CustomPainter {
  final double? at;
  _Progress(this.at);
  @override
  void paint(Canvas canvas, Size size) {
    final y = size.height / 2;
    canvas.drawRect(Rect.fromLTWH(0, y, size.width, 1), Paint()..color = line);
    final a = at;
    if (a == null) return;
    final x = size.width * a;
    canvas.drawRect(Rect.fromLTWH(0, y, x, 1), Paint()..color = bone);
    canvas.save();
    canvas.translate(x, y + 0.5);
    canvas.rotate(math.pi / 4);
    canvas.drawRect(const Rect.fromLTWH(-3, -3, 6, 6), Paint()..color = red);
    canvas.restore();
  }

  @override
  bool shouldRepaint(_Progress old) => old.at != at;
}
