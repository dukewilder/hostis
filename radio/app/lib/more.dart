// Pyrate Radio: the Log's filters, the watch bill, the tape, the hands and the ship's papers.
// No rights reserved.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'shell.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';

import 'art.dart';
import 'main.dart';
import 'native.dart';
import 'station.dart';

const ship = ShellChannel();

/// The kinds a station may call itself, for the Log's filters.
const stationKinds = ['talk', 'music', 'readings', 'news', 'sound'];

Widget pyrateButton(String label, VoidCallback? onTap, {Color color = bone}) => OutlinedButton(
      onPressed: onTap,
      style: OutlinedButton.styleFrom(
        side: BorderSide(color: onTap == null ? boneGhost : color.withValues(alpha: 0.7), width: 0.8),
        shape: const RoundedRectangleBorder(),
        padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 12),
      ),
      child: Text(label, style: carved(11, color: onTap == null ? boneGhost : color, spacing: 0.22)),
    );

InputDecoration pyrateField(String label) => InputDecoration(
      labelText: label,
      labelStyle: serif(15, color: boneFaint, italic: true),
      enabledBorder: const UnderlineInputBorder(borderSide: BorderSide(color: boneGhost)),
      focusedBorder: const UnderlineInputBorder(borderSide: BorderSide(color: bone)),
    );

/// A small hairline tag, the way the site marks LIVE and PREMIERE.
Widget tag(String text, Color color) => Container(
      padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1),
      decoration: BoxDecoration(border: Border.all(color: color, width: 1)),
      child: Text(text, style: caps(11, color: color, spacing: 0.18)),
    );

Widget note(String text) => Text(text, style: serif(14, color: boneFaint, italic: true));

Widget heading(String text, {Color color = bone}) => Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Text(text, style: TextStyle(fontFamily: 'Cinzel', fontWeight: FontWeight.w600, fontSize: 14, color: color, letterSpacing: 14 * 0.16, height: 1.2)),
    );

Widget pyrateSwitch(bool value, ValueChanged<bool> onChanged, String label) => Row(children: [
      Switch(value: value, onChanged: onChanged, activeThumbColor: bone, activeTrackColor: boneGhost, inactiveThumbColor: boneFaint, inactiveTrackColor: panel),
      Flexible(child: Padding(padding: const EdgeInsets.symmetric(vertical: 6), child: Text(label, style: serif(15, color: boneDim)))),
    ]);

Future<T?> pyrateSheet<T>(BuildContext context, Widget child) => showModalBottomSheet<T>(
      context: context,
      isScrollControlled: true,
      backgroundColor: panel,
      shape: const Border(top: BorderSide(color: line, width: 1)),
      // The sheet's edge wears the site's diamond where a toolkit would put a grab handle.
      builder: (c) => SafeArea(
          child: ConstrainedBox(
              constraints: BoxConstraints(maxHeight: MediaQuery.of(c).size.height * 0.85),
              child: Column(mainAxisSize: MainAxisSize.min, children: [const SizedBox(height: 12), const Diamond(size: 5), Flexible(child: child)]))),
    );

void toast(BuildContext context, String text) {
  ScaffoldMessenger.maybeOf(context)?.showSnackBar(SnackBar(
    backgroundColor: panel,
    content: Text(text, style: serif(14, color: bone)),
  ));
}

String clockTime(int ms) {
  final d = DateTime.fromMillisecondsSinceEpoch(ms);
  return '${d.hour.toString().padLeft(2, '0')}:${d.minute.toString().padLeft(2, '0')}';
}

String dayLabel(int ms) {
  final d = DateTime.fromMillisecondsSinceEpoch(ms);
  final today = DateTime.now();
  final a = DateTime(d.year, d.month, d.day);
  final b = DateTime(today.year, today.month, today.day);
  final diff = a.difference(b).inDays;
  if (diff == 0) return 'TODAY';
  if (diff == 1) return 'TOMORROW';
  const days = ['MONDAY', 'TUESDAY', 'WEDNESDAY', 'THURSDAY', 'FRIDAY', 'SATURDAY', 'SUNDAY'];
  return days[d.weekday - 1];
}

dynamic pref(String key, [dynamic value]) => Native.call({'cmd': 'pref', 'key': key, if (value != null) 'value': value});

/// HOSTIS's own stations, built into the app. Only these show the skull seal.
bool isHarbour(String id) => harbour.contains(id);

/// A station flying a harbour station's name without its key: false colours.
bool falseColours(Map s) {
  if (isHarbour(s['id'] as String)) return false;
  // Latin mixed with Greek or Cyrillic letters is how one name passes for another.
  if (s['mixed'] == true) return true;
  final name = (s['name'] ?? '').toString().trim().toLowerCase();
  return name.isNotEmpty && harbourNames.contains(name);
}

final Set<String> harbourNames = {};

/// The seal on a station's name: HOSTIS's own, or a warning.
List<Widget> seals(Map s, {double size = 9}) => [
      if (isHarbour(s['id'] as String)) ...[
        const SizedBox(width: 8),
        // HOSTIS's own stations wear the HOSTIS skull.
        Text('\u2620\uFE0E', style: TextStyle(fontFamily: 'Skull', fontSize: size * 1.9, color: bone, height: 1)),
      ],
      if (falseColours(s)) ...[
        const SizedBox(width: 8),
        Text('FALSE COLOURS?', style: carved(size, color: bone, spacing: 0.25)),
      ],
      if (s['explicit'] == true) ...[
        const SizedBox(width: 8),
        Text('EXPLICIT', style: carved(size, color: bone, spacing: 0.25)),
      ] else if (s['mature'] == true) ...[
        const SizedBox(width: 8),
        Text('MATURE', style: carved(size, color: boneFaint, spacing: 0.25)),
      ],
    ];

// ---------------------------------------------------------------------------------------
// The Log

class LogPage extends StatefulWidget {
  final Tuner radio;
  final VoidCallback onTune;
  const LogPage({super.key, required this.radio, required this.onTune});
  @override
  State<LogPage> createState() => _LogPageState();
}

class _LogPageState extends State<LogPage> {
  final search = TextEditingController();
  String sort = (pref('sort') as String?) ?? 'aboard';
  String? kind;
  String? port;
  bool hideMature = pref('hide_mature') == true;
  bool onNow = false;
  Map nowOn = {};
  int _nowTick = 0;
  @override
  void dispose() {
    search.dispose();
    super.dispose();
  }

  @override
  void initState() {
    super.initState();
    search.addListener(() => setState(() {}));
  }

  List<Map<String, dynamic>> filtered() {
    final q = search.text.trim().toLowerCase();
    var st = widget.radio.stations.where((s) {
      if (hideMature && (s['mature'] == true || s['explicit'] == true)) return false;
      if (kind != null && !((s['kinds'] as List?) ?? []).contains(kind)) return false;
      if (port != null && (s['port'] ?? '') != port) return false;
      if (onNow && s['live'] != true && ((nowOn[s['id']] as Map?)?['slot'] ?? '').toString().isEmpty) return false;
      if (q.isEmpty) return true;
      return [s['name'], s['about'], s['call_sign'], s['port']].any((v) => (v ?? '').toString().toLowerCase().contains(q));
    }).toList();
    int since(Map s) => (s['since'] as num?)?.toInt() ?? 0;
    int kept(Map s) => s['kept'] == true ? 1 : 0;
    switch (sort) {
      case 'longest':
        st.sort((a, b) => kept(b) != kept(a) ? kept(b) - kept(a) : since(a).compareTo(since(b)));
      case 'newest':
        st.sort((a, b) => kept(b) != kept(a) ? kept(b) - kept(a) : since(b).compareTo(since(a)));
      case 'carried':
        st.sort((a, b) => kept(b) != kept(a) ? kept(b) - kept(a) : ((b['carriers'] as num? ?? 0).compareTo(a['carriers'] as num? ?? 0)));
      default:
        break;
    }
    return st;
  }

  void refreshNow(List<Map<String, dynamic>> st) {
    if (_nowTick++ % 8 != 0) return;
    // On now looks across the whole Log; otherwise the stations in view are enough.
    final ids = (onNow ? widget.radio.stations : st).take(onNow ? 200 : 60).map((s) => s['id']).toList();
    final r = Native.call({'cmd': 'now_for', 'stations': ids});
    if (r is Map) nowOn = r;
  }

  Widget chip(String label, bool on, VoidCallback onTap) => Padding(
        padding: const EdgeInsets.only(right: 8, bottom: 8),
        child: InkWell(
          onTap: onTap,
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
            // The site's filter: a square hairline that turns to bone when it is on.
            color: on ? bone : Colors.transparent,
            decoration: null,
            foregroundDecoration: BoxDecoration(border: Border.all(color: on ? bone : line, width: 1)),
            child: Text(label.toUpperCase(), style: caps(11, color: on ? black : boneDim, spacing: 0.12)),
          ),
        ),
      );

  void longPress(BuildContext context, Map<String, dynamic> s) {
    final id = s['id'] as String;
    pyrateSheet(
      context,
      SingleChildScrollView(
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          Padding(padding: const EdgeInsets.fromLTRB(24, 20, 24, 4), child: Row(children: [Flexible(child: Text(s['name'] ?? '', style: pica(21, spacing: 0.02))), ...seals(s)])),
          Padding(padding: const EdgeInsets.fromLTRB(24, 0, 24, 8), child: Text('${s['call_sign']}  ·  Nothing here leaves this phone.', style: serif(13, color: boneFaint, italic: true))),
          ListTile(
            title: Text(s['kept'] == true ? 'STOP KEEPING' : 'KEEP', style: carved(12)),
            subtitle: Text('Kept stations sit at the top and are saved ahead on Wi-Fi.', style: serif(13, color: boneFaint, italic: true)),
            onTap: () {
              Native.callAway({'cmd': 'keep', 'station': id, 'value': s['kept'] != true});
              Navigator.pop(context);
            },
          ),
          ListTile(
            title: Text('STATION PAGE', style: carved(12)),
            subtitle: Text('Notices, what is on now and next, and the week\'s shows.', style: serif(13, color: boneFaint, italic: true)),
            onTap: () {
              Navigator.pop(context);
              Navigator.push(context, MaterialPageRoute(builder: (_) => StationPage(station: s, radio: widget.radio)));
            },
          ),
          ListTile(
            title: Text('SCHEDULE', style: carved(12)),
            subtitle: Text('What plays today and this week, with a reminder for any of it.', style: serif(13, color: boneFaint, italic: true)),
            onTap: () {
              Navigator.pop(context);
              pyrateSheet(context, WatchBill(station: s));
            },
          ),
          if (s['pocket'] != true) ListTile(
            title: Text('SHARE', style: carved(12)),
            subtitle: Text('A link that opens it in Pyrate Radio.', style: serif(13, color: boneFaint, italic: true)),
            onTap: () {
              Navigator.pop(context);
              shareStation(s);
            },
          ),
          if (s['pocket'] == true && id != widget.radio.mine?['id'])
            ListTile(
              title: Text('LEAVE THIS STATION', style: carved(12)),
              subtitle: Text('It goes from this phone. Only a new invitation brings it back.', style: serif(13, color: boneFaint, italic: true)),
              onTap: () {
                Native.callAway({'cmd': 'pocket_leave', 'station': id});
                if (widget.radio.tuned == id) widget.radio.stop();
                Navigator.pop(context);
              },
            ),
          ListTile(
            title: Text('HIDE THIS STATION', style: carved(12)),
            subtitle: Text('Gone from your Log and dial. Bring it back in Settings.', style: serif(13, color: boneFaint, italic: true)),
            onTap: () {
              Native.callAway({'cmd': 'maroon', 'station': id, 'value': true});
              if (widget.radio.tuned == id) widget.radio.stop();
              Navigator.pop(context);
            },
          ),
          const SizedBox(height: 10),
        ]),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final radio = widget.radio;
    final st = filtered();
    refreshNow(st);
    final ports = radio.stations.map((s) => (s['port'] ?? '').toString()).where((p) => p.isNotEmpty).toSet().toList()..sort();
    return CustomScrollView(slivers: [
      SliverToBoxAdapter(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(24, 26, 24, 6),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            masthead('Stations'),
            const SizedBox(height: 8),
            note('Every station on the air, the ones you keep first. Tap one to listen.'),
            if (radio.carrying > 0)
              Padding(
                padding: const EdgeInsets.only(top: 6),
                child: Text('This device helps keep ${radio.carrying} ${radio.carrying == 1 ? 'station' : 'stations'} on the air.', style: mono(11, color: boneFaint)),
              ),
            const SizedBox(height: 10),
            Row(children: [
              Expanded(child: TextField(enableIMEPersonalizedLearning: false, controller: search, style: serif(17), decoration: pyrateField('Find a station'))),
              const SizedBox(width: 12),
              IconButton(
                tooltip: 'Stations on a map',
                icon: const Icon(Icons.public, size: 22, color: boneDim),
                onPressed: () => Navigator.push(context, MaterialPageRoute(builder: (_) => ChartPage(radio: radio, onTune: widget.onTune))),
              ),
              pyrateButton('SPIN THE DIAL', () {
                final pool = filtered().where((s) => s['id'] != radio.tuned).toList();
                if (pool.isEmpty) return;
                final s = pool[DateTime.now().microsecondsSinceEpoch % pool.length];
                radio.tune(s['id'] as String);
                widget.onTune();
              }),
            ]),
            const SizedBox(height: 12),
            // The order, then the filters, each a row that slides sideways so the list itself
            // stays near the top of the screen.
            SizedBox(height: 46, child: ListView(scrollDirection: Axis.horizontal, children: [
              for (final (k, label) in [('aboard', 'Most listened'), ('longest', 'Longest on air'), ('newest', 'Newest'), ('carried', 'Most backed up')])
                chip(label, sort == k, () => setState(() {
                      sort = k;
                      pref('sort', k);
                    })),
            ])),
            SizedBox(height: 46, child: ListView(scrollDirection: Axis.horizontal, children: [
              chip('on now', onNow, () => setState(() {
                    onNow = !onNow;
                    _nowTick = 0;
                  })),
              chip('hide mature', hideMature, () => setState(() {
                    hideMature = !hideMature;
                    pref('hide_mature', hideMature);
                  })),
              for (final k in stationKinds) chip(k, kind == k, () => setState(() => kind = kind == k ? null : k)),
              for (final p in ports) chip(p, port == p, () => setState(() => port = port == p ? null : p)),
            ])),
            const SizedBox(height: 6),
            const Divider(color: lineSoft, height: 1, thickness: 1),
          ]),
        ),
      ),
      if (st.isEmpty)
        SliverToBoxAdapter(
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Text(radio.stations.isEmpty ? 'Finding stations on the air' : onNow ? 'No shows and no one live at this minute. Spin the dial.' : 'No station answers to that.', style: serif(16, color: boneFaint, italic: true)),
          ),
        ),
      SliverList.separated(
        itemCount: st.length,
        separatorBuilder: (_, __) => const Padding(padding: EdgeInsets.symmetric(horizontal: 24), child: Divider(color: boneGhost, height: 1, thickness: 0.5)),
        itemBuilder: (context, i) {
          final s = st[i];
          final id = s['id'] as String;
          final live = s['live'] == true;
          final tuned = radio.tuned == id && radio.playing;
          final on = nowOn[id] as Map?;
          return InkWell(
            onTap: () {
              radio.tune(id);
              widget.onTune();
            },
            onLongPress: () => longPress(context, s),
            child: Container(
              // The station in hand is marked as the site marks a row: a red rule down its edge.
              decoration: BoxDecoration(
                color: tuned ? const Color(0x0AC52828) : null,
                border: tuned ? const Border(left: BorderSide(color: red, width: 2)) : null,
              ),
              padding: EdgeInsets.fromLTRB(tuned ? 22 : 24, 12, 8, 12),
              child: Row(children: [
                StationArt(station: id, hash: s['cover'], width: 52, height: 52, fallback: Flag(id: id, width: 48, live: live)),
                const SizedBox(width: 16),
                Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Row(children: [
                      Flexible(child: Text(s['name'] ?? '', maxLines: 1, overflow: TextOverflow.ellipsis, style: pica(18, color: tuned ? bone : boneDim, spacing: 0.02))),
                      if (live) ...[const SizedBox(width: 8), tag('ON AIR', bloodBright)],
                      if (s['kept'] == true) ...[const SizedBox(width: 8), tag('KEPT', boneDim)],
                      if (s['pocket'] == true) ...[const SizedBox(width: 8), tag('PRIVATE', boneFaint)],
                      ...seals(s),
                    ]),
                    const SizedBox(height: 2),
                    if ((s['call_sign'] ?? '').toString().isNotEmpty) Text(s['call_sign'] ?? '', style: caps(11, color: boneFaint, spacing: 0.12)),
                    if (on != null && (on['slot'] ?? '').toString().isNotEmpty)
                      Text((on['slot'] as String).toUpperCase(), maxLines: 1, overflow: TextOverflow.ellipsis, style: caps(11, color: boneDim, spacing: 0.18)),
                    if (on != null && (on['title'] ?? '').toString().isNotEmpty)
                      Text('On now: ${on['title']}${(on['by'] ?? '').toString().isNotEmpty ? ' · ${on['by']}' : ''}',
                          maxLines: 1, overflow: TextOverflow.ellipsis, style: serif(14, color: boneDim, italic: true))
                    else if ((s['about'] ?? '').toString().isNotEmpty)
                      Text(s['about'], maxLines: 2, overflow: TextOverflow.ellipsis, style: serif(14, color: boneFaint, italic: true)),
                  ]),
                ),
                const SizedBox(width: 8),
                if (s['placeholder'] != true)
                  Column(crossAxisAlignment: CrossAxisAlignment.end, children: [
                    Text('${s['aboard'] ?? 0}', style: mono(16, color: tuned ? bone : boneDim)),
                    Text('LISTENING', style: caps(11, color: boneFaint, spacing: 0.1)),
                  ]),
                // The rest of what can be done with a station, without a long press to find it.
                IconButton(
                  tooltip: 'More for ${s['name'] ?? 'this station'}',
                  icon: const Icon(Icons.more_vert, size: 20, color: boneFaint),
                  onPressed: () => longPress(context, s),
                ),
              ]),
            ),
          );
        },
      ),
      const SliverToBoxAdapter(child: SizedBox(height: 30)),
    ]);
  }
}

// ---------------------------------------------------------------------------------------
// The watch bill and its bells

/// A bell's id: the same for a show on every screen and in every build, from the station
/// and the minute it starts (FNV-1a, not the toolkit's hash, which may change between builds).
int bellId(String station, int start) {
  var h = 0x811c9dc5;
  for (final b in utf8.encode('$station@${start ~/ 60000}')) {
    h = ((h ^ b) * 0x01000193) & 0xffffffff;
  }
  return h & 0x7fffffff;
}

/// The ids of the bells already set, worked out afresh from what each is for.
Set<int> bellIds(String json) {
  try {
    return (jsonDecode(json) as List).map((x) => bellId((x['station'] ?? '').toString(), (x['at'] as num).toInt())).toSet();
  } catch (_) {
    return {};
  }
}

/// Sets a bell, and says so at once if the phone would never show it.
Future<void> ringAt(BuildContext context, {required int id, required int at, required String title, required String station, required String name}) async {
  final ok = await ship.invokeMethod<bool>('bell_set', {'id': id, 'at': at, 'title': title, 'station': station, 'name': name}).catchError((_) => true) ?? true;
  if (ok || !context.mounted) return;
  final go = await showDialog<bool>(
    context: context,
    builder: (c) => AlertDialog(
      backgroundColor: panel,
      shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
      title: Text('The reminder is set, but it cannot ring', style: serif(20)),
      content: Text('Notifications are off for the app. Turn them on in your phone\'s settings and the reminder rings when the show starts.', style: serif(14, color: boneFaint, italic: true)),
      actions: [
        TextButton(onPressed: () => Navigator.pop(c, false), child: Text('NOT NOW', style: carved(11, color: boneFaint))),
        TextButton(onPressed: () => Navigator.pop(c, true), child: Text('OPEN SETTINGS', style: carved(11))),
      ],
    ),
  );
  if (go == true) ship.invokeMethod('app_settings').catchError((_) {});
}

class WatchBill extends StatefulWidget {
  final Map<String, dynamic> station;
  const WatchBill({super.key, required this.station});
  @override
  State<WatchBill> createState() => _WatchBillState();
}

class _WatchBillState extends State<WatchBill> {
  List bill = [];
  Set<int> bells = {};
  String? error;
  bool loading = true;

  @override
  void initState() {
    super.initState();
    load();
  }

  Future<void> load() async {
    final r = await Native.callAway({'cmd': 'schedule', 'station': widget.station['id'], 'hours': 72});
    final b = await ship.invokeMethod<String>('bells').catchError((_) => '[]') ?? '[]';
    if (mounted) setState(() {
      loading = false;
      if (r is List) {
        bill = r;
      } else {
        error = (r is Map ? r['error'] : null) ?? 'The schedule has not arrived yet.';
      }
      bells = bellIds(b);
    });
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.station;
    String? lastDay;
    return Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(24, 22, 24, 4),
        child: Text('SCHEDULE', style: carved(16)),
      ),
      Padding(
        padding: const EdgeInsets.fromLTRB(24, 0, 24, 10),
        child: note('${s['name']}. Set a reminder and the phone tells you when a show starts.'),
      ),
      if (loading) Padding(padding: const EdgeInsets.all(24), child: note('Loading the schedule')),
      if (error != null) Padding(padding: const EdgeInsets.all(24), child: note(error!)),
      Flexible(
        child: ListView(shrinkWrap: true, padding: const EdgeInsets.fromLTRB(24, 0, 12, 24), children: [
          for (final item in bill) ...[
            if (dayLabel((item['start_ms'] as num).toInt()) != lastDay)
              Builder(builder: (_) {
                lastDay = dayLabel((item['start_ms'] as num).toInt());
                return Padding(padding: const EdgeInsets.only(top: 14, bottom: 4), child: Text(lastDay!, style: carved(11, color: boneFaint, spacing: 0.3)));
              }),
            Builder(builder: (_) {
              final start = (item['start_ms'] as num).toInt();
              final id = bellId(s['id'] as String, start);
              final ringing = bells.contains(id);
              final past = start < DateTime.now().millisecondsSinceEpoch;
              return Row(children: [
                SizedBox(width: 54, child: Text(clockTime(start), style: mono(12, color: past ? bone : boneFaint, weight: past ? FontWeight.w700 : FontWeight.w400))),
                Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text(item['title'] ?? '', maxLines: 1, overflow: TextOverflow.ellipsis, style: serif(15, color: past ? bone : boneDim)),
                    if ((item['by'] ?? '').toString().isNotEmpty) Text(item['by'], style: carved(11, color: boneFaint, spacing: 0.2)),
                  ]),
                ),
                if (!past)
                  IconButton(
                    tooltip: ringing ? 'Cancel the reminder' : 'Remind me when it starts',
                    icon: Icon(ringing ? Icons.notifications_active : Icons.notifications_none, size: 20, color: ringing ? bone : boneFaint),
                    onPressed: () async {
                      if (ringing) {
                        await ship.invokeMethod('bell_cancel', {'id': id, 'station': s['id'], 'at': start});
                        if (mounted) setState(() => bells.remove(id));
                      } else {
                        await ringAt(context, id: id, at: start, title: (item['title'] ?? 'A show').toString(), station: s['id'] as String, name: (s['name'] ?? '').toString());
                        if (mounted) setState(() => bells.add(id));
                      }
                    },
                  )
                else
                  const SizedBox(width: 48, child: Center(child: Text('NOW', style: TextStyle(fontFamily: 'Cinzel', fontSize: 9, color: bone, letterSpacing: 2)))),
              ]);
            }),
          ],
        ]),
      ),
    ]);
  }
}

// ---------------------------------------------------------------------------------------
// Tape it, pass it on, tip

String safeName(String s) => s.replaceAll(RegExp(r'[^\w\s\-.,()]'), '').trim();

Future<void> tapeIt(BuildContext context) async {
  final dir = await scratchDir();
  final share = Directory('${dir.path}/share');
  if (!share.existsSync()) share.createSync(recursive: true);
  final tmp = '${share.path}/tape-${DateTime.now().millisecondsSinceEpoch}.ogg';
  final r = await Native.callAway({'cmd': 'tape', 'seconds': 120, 'path': tmp});
  if (r is! Map || r['error'] != null) {
    if (context.mounted) toast(context, 'Nothing to record yet. Let it play a moment.');
    return;
  }
  final name = '${share.path}/${safeName('${r['station']} - ${r['title']}').isEmpty ? 'Pyrate Radio' : safeName('${r['station']} - ${r['title']}')}.ogg';
  File(tmp).renameSync(name);
  await ship.invokeMethod('share_file', {'path': name, 'mime': 'audio/ogg'});
}

/// One fetch from the open web, made by this phone itself: a plain client name rather than
/// the toolkit's own, timeouts on connecting and on every gap, a cap on size, and a file
/// that only takes its name once whole. The path, or null.
Future<String?> fetchOut(String url, {int maxBytes = 30000000}) async {
  final uri = Uri.tryParse(url.trim());
  if (uri == null || !(uri.scheme == 'https' || uri.scheme == 'http') || uri.host.isEmpty) return null;
  final client = HttpClient()
    ..connectionTimeout = const Duration(seconds: 20)
    ..idleTimeout = const Duration(seconds: 15)
    ..userAgent = 'Mozilla/5.0';
  final dir = await scratchDir();
  final stem = '${dir.path}/out-${DateTime.now().microsecondsSinceEpoch}';
  final part = File('$stem.part');
  try {
    final rq = await client.getUrl(uri).timeout(const Duration(seconds: 30));
    rq.followRedirects = true;
    rq.maxRedirects = 6;
    final rs = await rq.close().timeout(const Duration(seconds: 60));
    if (rs.statusCode != 200 || rs.contentLength > maxBytes) return null;
    final sink = part.openWrite();
    var n = 0;
    try {
      await for (final chunk in rs.timeout(const Duration(seconds: 60))) {
        n += chunk.length;
        if (n > maxBytes) throw 'too large';
        sink.add(chunk);
      }
    } finally {
      await sink.close();
    }
    return part.renameSync('$stem.got').path;
  } catch (_) {
    return null;
  } finally {
    client.close(force: true);
    try {
      if (part.existsSync()) part.deleteSync();
    } catch (_) {}
  }
}

/// Asks for the microphone. True once it is allowed; otherwise says why on the spot and,
/// when only the phone's settings can allow it now, offers to open them.
Future<bool> askMic(BuildContext context) async {
  final r = await ship.invokeMethod<String>('mic').catchError((_) => 'denied') ?? 'denied';
  if (r == 'ok') return true;
  if (!context.mounted) return false;
  if (r == 'blocked') {
    final go = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        backgroundColor: panel,
        shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
        title: Text('The microphone is shut', style: serif(20)),
        content: Text('This phone will not ask again. Allow the microphone for the app in its settings, then come back and press again.', style: serif(14, color: boneFaint, italic: true)),
        actions: [
          TextButton(onPressed: () => Navigator.pop(c, false), child: Text('NOT NOW', style: carved(11, color: boneFaint))),
          TextButton(onPressed: () => Navigator.pop(c, true), child: Text('OPEN SETTINGS', style: carved(11))),
        ],
      ),
    );
    if (go == true) ship.invokeMethod('app_settings').catchError((_) {});
  } else {
    toast(context, 'Recording needs the microphone. Press again to be asked again.');
  }
  return false;
}

/// Removes a file, but only one inside this app's own cache: a copy, a take or a fetch,
/// never something the owner keeps elsewhere.
Future<void> dropCopy(String? path) async {
  if (path == null) return;
  try {
    final tmp = (await scratchDir()).path;
    if (File(path).absolute.path.startsWith('$tmp/')) await File(path).delete();
  } catch (_) {}
}

/// Sets a recording on a station, one after another rather than all at once, and drops
/// the cache copy once it is on the clock or has failed. The job's last word, or null.
Future<String?> setTrack(Map<String, dynamic> req) => _setQueue = _setQueue.then((_) async {
      final r = await Native.callAway({'cmd': 'add_track', ...req});
      final job = r is Map ? r['job'] : null;
      String? last;
      for (var i = 0; job != null && i < 1200; i++) {
        await Future.delayed(const Duration(seconds: 2));
        final st = await Native.callAway({'cmd': 'status'});
        final j = st is Map ? ((st['jobs'] as List?) ?? []).firstWhere((x) => x['id'] == job, orElse: () => null) : null;
        last = j?['state'] as String?;
        if (j == null || !(last ?? '').startsWith('Setting')) break;
      }
      await dropCopy(req['path'] as String?);
      return last;
    });
Future<String?> _setQueue = Future.value(null);

/// Clears what earlier runs fetched, recorded or copied and never cleaned up: a fetch cut
/// off by a closed app, a take never set, a picked file, a shared tape, the app's own copy
/// made for passing it on.
Future<void> sweepTemp() async {
  try {
    final dir = await scratchDir();
    final old = DateTime.now().subtract(const Duration(hours: 6));
    const prefixes = ['out-', 'feed-', 'pyrate-art-', 'rec-', 'voice-'];
    await for (final e in dir.list(recursive: true)) {
      if (e is! File) continue;
      final rel = e.path.substring(dir.path.length + 1);
      final mine = rel.startsWith('share/') || rel.startsWith('file_picker/') || (!rel.contains('/') && prefixes.any(rel.startsWith));
      if (mine && e.statSync().modified.isBefore(old)) {
        try {
          e.deleteSync();
        } catch (_) {}
      }
    }
  } catch (_) {}
}

void shareStation(Map s) {
  if (s['pocket'] == true) return;
  ship.invokeMethod('share_text', {
    'text': 'Tune in to ${s['name']} on Pyrate Radio: ${stationLink(s['id'] as String)}',
  });
}

/// Tips go from the listener's own wallet to the station. The app moves no money.
/// The wallet link for a tip address, with a note where the kind of address carries one:
/// a Bitcoin payment's message, a Monero payment's description. A Lightning address takes
/// its note in the wallet's own comment box.
String tipUri(String addr, String note) {
  final a = addr.trim();
  final n = Uri.encodeComponent(note.trim());
  if (a.startsWith('lightning:') || a.startsWith('bitcoin:') || a.startsWith('monero:')) return a;
  if (a.contains('@') || a.toLowerCase().startsWith('lnurl') || a.toLowerCase().startsWith('lno')) return 'lightning:$a';
  if ((a.startsWith('4') || a.startsWith('8')) && (a.length == 95 || a.length == 106)) return 'monero:$a${n.isEmpty ? '' : '?tx_description=$n'}';
  return 'bitcoin:$a${n.isEmpty ? '' : '?message=$n'}';
}

/// Tips go from the listener's own wallet to the station, with a note if they like. The app
/// moves no money and sees none of it.
Future<void> tipStation(BuildContext context, String addr, {String name = 'the station'}) => pyrateSheet(context, TipSheet(addr: addr, name: name));

class TipSheet extends StatefulWidget {
  final String addr;
  final String name;
  const TipSheet({super.key, required this.addr, required this.name});
  @override
  State<TipSheet> createState() => _TipSheetState();
}

class _TipSheetState extends State<TipSheet> {
  final memo = TextEditingController();

  bool get lightning => tipUri(widget.addr, '').startsWith('lightning:');
  @override
  void dispose() {
    memo.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Padding(
        padding: EdgeInsets.fromLTRB(24, 20, 24, 24 + MediaQuery.of(context).viewInsets.bottom),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          heading('A TIP FOR ${widget.name.toUpperCase()}'),
          note('It goes from your own wallet straight to the station. Pyrate Radio moves no money and sees none of it.'),
          const SizedBox(height: 10),
          SelectableText(widget.addr, style: mono(11, color: boneDim)),
          TextField(enableIMEPersonalizedLearning: false, controller: memo, maxLength: 140, style: serif(16), decoration: pyrateField(lightning ? 'A note for the keeper (paste it in your wallet\'s comment)' : 'A note for the keeper')),
          Wrap(spacing: 6, runSpacing: 6, children: [
            pyrateButton('OPEN MY WALLET', () async {
              if (lightning && memo.text.trim().isNotEmpty) await copySecret(memo.text.trim());
              final ok = await ship.invokeMethod<bool>('open_uri', {'uri': tipUri(widget.addr, memo.text)}).catchError((_) => false) ?? false;
              if (!context.mounted) return;
              if (!ok) {
                await copySecret(widget.addr);
                if (context.mounted) toast(context, 'No wallet on this phone answered. The address is copied.');
              } else if (lightning && memo.text.trim().isNotEmpty) {
                toast(context, 'Your note is copied. Paste it in the wallet\'s comment.');
              }
            }),
            pyrateButton('COPY THE ADDRESS', () async {
              await copySecret(widget.addr);
              if (context.mounted) toast(context, 'The address is copied.');
            }),
          ]),
        ]),
      );
}

class PassItOn extends StatefulWidget {
  const PassItOn({super.key});
  @override
  State<PassItOn> createState() => _PassItOnState();
}

class _PassItOnState extends State<PassItOn> {
  String? url;
  Map? qr;

  @override
  void initState() {
    super.initState();
    ship.invokeMethod<String>('pass_start').then((u) {
      if (!mounted) return;
      setState(() {
        url = u;
        if (u != null) qr = Native.call({'cmd': 'qr', 'text': u}) as Map?;
      });
    }).catchError((_) {});
  }

  @override
  void dispose() {
    ship.invokeMethod('pass_stop').catchError((_) {});
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(24, 22, 24, 24),
      child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text('PASS IT ON', style: carved(16)),
        const SizedBox(height: 8),
        note('Hand the radio itself to someone beside you. Point their camera at the code while you are on the same Wi-Fi or hotspot, or send it by Quick Share. It needs no internet.'),
        const SizedBox(height: 18),
        if (qr != null && qr!['bits'] != null)
          Center(
            child: Container(
              color: bone,
              padding: const EdgeInsets.all(14),
              child: SizedBox(width: 220, height: 220, child: CustomPaint(painter: QrPainter((qr!['size'] as num).toInt(), qr!['bits'] as String))),
            ),
          )
        else
          note(url == null ? 'Join a Wi-Fi network or turn on your hotspot to share by code.' : 'Drawing the code'),
        if (url != null) Padding(padding: const EdgeInsets.only(top: 10), child: Center(child: Text(url!, style: mono(11)))),
        const SizedBox(height: 18),
        pyrateButton('SEND BY QUICK SHARE OR BLUETOOTH', () => ship.invokeMethod('share_app')),
      ]),
    );
  }
}

class QrPainter extends CustomPainter {
  final int n;
  final String bits;
  QrPainter(this.n, this.bits);
  @override
  void paint(Canvas canvas, Size size) {
    final cell = size.width / n;
    final p = Paint()..color = black;
    for (var i = 0; i < bits.length; i++) {
      if (bits[i] == '1') canvas.drawRect(Rect.fromLTWH((i % n) * cell, (i ~/ n) * cell, cell + 0.5, cell + 0.5), p);
    }
  }

  @override
  bool shouldRepaint(QrPainter old) => old.bits != bits;
}

// ---------------------------------------------------------------------------------------
// Sleep and history

Future<void> sleepSheet(BuildContext context, Tuner radio) => pyrateSheet(
      context,
      Padding(
        padding: const EdgeInsets.fromLTRB(24, 22, 24, 24),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text('SLEEP TIMER', style: carved(16)),
          const SizedBox(height: 8),
          note(radio.sleepAt == null ? 'The radio fades and stops on its own.' : 'Fades out at ${clockTime(radio.sleepAt!.millisecondsSinceEpoch)}.'),
          const SizedBox(height: 14),
          Wrap(spacing: 10, runSpacing: 10, children: [
            for (final m in [15, 30, 45, 60, 90])
              pyrateButton('$m MIN', () {
                radio.sleepIn(Duration(minutes: m));
                Navigator.pop(context);
              }),
            if (radio.sleepAt != null)
              pyrateButton('CANCEL', () {
                radio.sleepIn(null);
                Navigator.pop(context);
              }, color: boneFaint),
          ]),
        ]),
      ),
    );

Future<void> historySheet(BuildContext context, Tuner radio) => pyrateSheet(
      context,
      Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
        Padding(padding: const EdgeInsets.fromLTRB(24, 22, 24, 4), child: Text('RECENTLY PLAYED', style: carved(16))),
        Padding(padding: const EdgeInsets.fromLTRB(24, 0, 24, 10), child: note('Forgotten when the app closes. Use Record this to keep something.')),
        Flexible(
          child: ListView(shrinkWrap: true, padding: const EdgeInsets.fromLTRB(24, 0, 24, 24), children: [
            if (radio.heard.isEmpty) note('Nothing yet.'),
            for (final h in radio.heard.reversed)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 5),
                child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  SizedBox(width: 54, child: Text(clockTime(h['at'] as int), style: mono(11))),
                  Expanded(
                    child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                      Text(h['title'] ?? '', style: serif(15, color: boneDim)),
                      Text('${h['by'] ?? ''}${(h['by'] ?? '').toString().isNotEmpty ? ' · ' : ''}${h['station'] ?? ''}', style: carved(11, color: boneFaint, spacing: 0.2)),
                    ]),
                  ),
                ]),
              ),
          ]),
        ),
      ]),
    );

// ---------------------------------------------------------------------------------------
// The captain's panels on the Colours page

class RecordPanel extends StatefulWidget {
  final String station;
  const RecordPanel({super.key, required this.station});
  @override
  State<RecordPanel> createState() => _RecordPanelState();
}

class _RecordPanelState extends State<RecordPanel> {
  bool recording = false;
  String? path;
  int ms = 0;
  DateTime? started;
  Timer? tick;
  final title = TextEditingController();

  @override
  void dispose() {
    tick?.cancel();
    // Closing the panel stops the microphone at once; a take never set goes with it.
    if (recording) {
      ship.invokeMethod<Map>('record_stop').then((r) => dropCopy(r?['path'] as String?)).catchError((_) {});
    } else {
      dropCopy(path);
    }
    title.dispose();
    super.dispose();
  }

  Future<void> toggle() async {
    if (!recording) {
      if (!await askMic(context)) return;
      final p = await ship.invokeMethod<String>('record_start').catchError((_) => null);
      if (p == null) {
        if (mounted) toast(context, 'The microphone would not start. Another app may have it.');
        return;
      }
      setState(() {
        recording = true;
        path = null;
        started = DateTime.now();
      });
      tick = Timer.periodic(const Duration(seconds: 1), (_) => setState(() {}));
    } else {
      tick?.cancel();
      final r = await ship.invokeMethod<Map>('record_stop').catchError((_) => <dynamic, dynamic>{}) ?? {};
      if (!mounted) {
        dropCopy(r['path'] as String?);
        return;
      }
      setState(() {
        recording = false;
        path = r['path'] as String?;
        ms = (r['ms'] as num?)?.toInt() ?? 0;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final running = started == null ? 0 : DateTime.now().difference(started!).inMilliseconds;
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      heading('RECORD SOMETHING'),
      note('Talk into the phone. A show, a reading, or a few words between songs.'),
      const SizedBox(height: 12),
      Row(children: [
        pyrateButton(recording ? '■︎  STOP  ${fmtTime(running)}' : '●︎  RECORD', toggle, color: recording ? bloodBright : bone),
      ]),
      if (path != null) ...[
        const SizedBox(height: 10),
        Text('${fmtTime(ms)} recorded', style: mono(11)),
        TextField(enableIMEPersonalizedLearning: false, controller: title, style: serif(17), decoration: pyrateField('Title')),
        const SizedBox(height: 10),
        Row(children: [
          pyrateButton('ADD IT TO THE STATION', () {
            setTrack({
              'station': widget.station,
              'path': path,
              'title': title.text.trim().isEmpty ? 'Segment, ${clockTime(DateTime.now().millisecondsSinceEpoch)}' : title.text.trim(),
              'talk': true,
            });
            setState(() {
              path = null;
              title.clear();
            });
          }),
          const SizedBox(width: 10),
          pyrateButton('DISCARD', () {
            dropCopy(path);
            setState(() => path = null);
          }, color: boneFaint),
        ]),
      ],
    ]);
  }
}

class DetailsPanel extends StatefulWidget {
  final String station;
  const DetailsPanel({super.key, required this.station});
  @override
  State<DetailsPanel> createState() => _DetailsPanelState();
}

class _DetailsPanelState extends State<DetailsPanel> {
  Set<String> kinds = {};
  bool mature = false;
  bool explicit = false;
  final port = TextEditingController();
  final tip = TextEditingController();
  String? saved;
  @override
  void dispose() {
    port.dispose();
    tip.dispose();
    super.dispose();
  }

  @override
  void initState() {
    super.initState();
    final d = Native.call({'cmd': 'details', 'station': widget.station});
    if (d is Map && d['error'] == null) {
      kinds = ((d['kinds'] as List?) ?? []).cast<String>().toSet();
      mature = d['mature'] == true;
      explicit = d['explicit'] == true;
      port.text = d['port'] ?? '';
      tip.text = d['tip'] ?? '';
    }
  }

  @override
  Widget build(BuildContext context) {
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      heading('ABOUT THE STATION'),
      note('Helps listeners find you in Stations.'),
      const SizedBox(height: 10),
      Wrap(spacing: 8, runSpacing: 8, children: [
        for (final k in stationKinds)
          InkWell(
            onTap: () => setState(() => kinds.contains(k) ? kinds.remove(k) : kinds.add(k)),
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
              decoration: BoxDecoration(border: Border.all(color: kinds.contains(k) ? bone : boneGhost, width: kinds.contains(k) ? 0.9 : 0.5)),
              child: Text(k.toUpperCase(), style: carved(11, color: kinds.contains(k) ? bone : boneFaint, spacing: 0.22)),
            ),
          ),
      ]),
      const SizedBox(height: 6),
      setting('Mature', mature, (v) => setState(() => mature = v), sub: 'Strong language or themes. Hidden from listeners who hide mature stations.'),
      setting('Explicit', explicit, (v) => setState(() => explicit = v), sub: 'Sexual or graphic material. Only listeners who choose to see explicit stations will.'),
      TextField(enableIMEPersonalizedLearning: false, controller: port, style: serif(16), decoration: pyrateField('Town or city (optional)')),
      note('Everyone can see it. Leave it empty, or pick a city that is not yours.'),
      TextField(enableIMEPersonalizedLearning: false, controller: tip, style: mono(12, color: bone), decoration: pyrateField('Tips: a Lightning, Bitcoin or Monero address')),
      note('Anyone can look up Bitcoin payments. Monero keeps them private.'),
      const SizedBox(height: 12),
      Row(children: [
        pyrateButton('SAVE', () {
          Native.call({'cmd': 'details_set', 'station': widget.station, 'kinds': kinds.toList(), 'mature': mature, 'explicit': explicit});
          final r = Native.call({'cmd': 'colours_set', 'station': widget.station, 'port': port.text, 'tip': tip.text});
          setState(() => saved = r is Map && r['error'] != null ? r['error'] : 'Saved. Listeners see it within a minute.');
        }),
      ]),
      if (saved != null) Padding(padding: const EdgeInsets.only(top: 8), child: note(saved!)),
    ]);
  }
}

class AboardChart extends StatelessWidget {
  final String station;
  const AboardChart({super.key, required this.station});
  @override
  Widget build(BuildContext context) {
    final r = Native.call({'cmd': 'aboard_history', 'station': station});
    final pts = r is List ? r.map((p) => ((p as List)[0] as num, p[1] as num)).toList() : <(num, num)>[];
    // A new station has nothing to chart yet, and an empty chart is only noise.
    if (pts.length < 2) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.only(top: 18),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        heading('LISTENERS, HOUR BY HOUR'),
        SizedBox(height: 70, width: double.infinity, child: CustomPaint(painter: _Spark(pts.map((p) => p.$2.toDouble()).toList()))),
        const SizedBox(height: 4),
        Text('most ${pts.map((p) => p.$2).reduce((a, b) => a > b ? a : b)} · now ${pts.last.$2}', style: mono(11)),
      ]),
    );
  }
}

class _Spark extends CustomPainter {
  final List<double> v;
  _Spark(this.v);
  @override
  void paint(Canvas canvas, Size size) {
    final top = v.reduce((a, b) => a > b ? a : b).clamp(1, double.infinity);
    final path = Path();
    for (var i = 0; i < v.length; i++) {
      final x = size.width * i / (v.length - 1);
      final y = size.height - size.height * v[i] / top;
      i == 0 ? path.moveTo(x, y) : path.lineTo(x, y);
    }
    canvas.drawPath(path, Paint()
      ..color = bone
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1);
  }

  @override
  bool shouldRepaint(_Spark old) => true;
}

/// Co-hosts. A station's master words appoint another phone's key as a deputy.
class HandsPanel extends StatefulWidget {
  final String? station;
  const HandsPanel({super.key, this.station});
  @override
  State<HandsPanel> createState() => _HandsPanelState();
}

class _HandsPanelState extends State<HandsPanel> {
  String? myCode;
  String? msg;

  Future<void> appoint() async {
    final words = TextEditingController();
    final wpass = TextEditingController();
    final code = TextEditingController();
    final label = TextEditingController();
    await secure(true);
    if (!mounted) return;
    final ok = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        backgroundColor: panel,
        shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
        title: Text('ADD A CO-HOST', style: carved(13)),
        content: SingleChildScrollView(
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            note('Ask your co-host to open My Station on their phone, press BE A CO-HOST and send you the code it shows.'),
            TextField(enableIMEPersonalizedLearning: false, controller: code, style: mono(12, color: bone), decoration: pyrateField('Their code')),
            TextField(enableIMEPersonalizedLearning: false, controller: label, style: serif(16), decoration: pyrateField('Their name, for your own record')),
            TextField(controller: words, style: mono(12, color: bone), maxLines: 3, decoration: pyrateField('Your 24 words'),
                enableSuggestions: false, autocorrect: false, enableIMEPersonalizedLearning: false, keyboardType: TextInputType.visiblePassword),
            TextField(controller: wpass, obscureText: true, style: mono(12, color: bone), decoration: pyrateField('Passphrase, if you set one'),
                enableSuggestions: false, autocorrect: false, enableIMEPersonalizedLearning: false),
          ]),
        ),
        actions: [TextButton(onPressed: () => Navigator.pop(c, true), child: Text('ADD', style: carved(11)))],
      ),
    );
    await secure(false);
    final phrase = words.text, pp = wpass.text;
    words.clear();
    wpass.clear();
    if (ok != true) return;
    final r = await Native.callAway({'cmd': 'hand_appoint', 'station': widget.station, 'words': phrase, 'passphrase': pp, 'code': code.text, 'label': label.text});
    if (!mounted) return;
    setState(() => msg = r is Map && r['error'] != null ? r['error'] : 'Added. Send them your station\'s id, shown below.');
  }

  Future<void> join() async {
    final r = Native.call({'cmd': 'hand_request'});
    setState(() => myCode = r is Map ? r['code'] : null);
  }

  Future<void> board() async {
    final id = TextEditingController();
    final ok = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        backgroundColor: panel,
        shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
        title: Text('JOIN AS A CO-HOST', style: carved(13)),
        content: TextField(enableIMEPersonalizedLearning: false, controller: id, style: mono(12, color: bone), decoration: pyrateField('The station\'s id, from its owner')),
        actions: [TextButton(onPressed: () => Navigator.pop(c, true), child: Text('JOIN', style: carved(11)))],
      ),
    );
    if (ok != true) return;
    final r = Native.call({'cmd': 'hand_join', 'station': id.text.trim()});
    setState(() => msg = r is Map && r['error'] != null ? r['error'] : 'Joined. The station arrives within a minute.');
  }

  @override
  Widget build(BuildContext context) {
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      heading('CO-HOSTS'),
      note('Let someone broadcast for your station from their own phone. Only your 24 words can add or remove a co-host.'),
      const SizedBox(height: 12),
      Wrap(spacing: 10, runSpacing: 10, children: [
        if (widget.station != null) pyrateButton('ADD A CO-HOST', appoint),
        if (widget.station == null) pyrateButton('BE A CO-HOST', join),
        if (myCode != null) pyrateButton('I HAVE BEEN ADDED', board),
      ]),
      if (widget.station != null)
        Padding(padding: const EdgeInsets.only(top: 10), child: SelectableText('This station\'s id: ${widget.station}', style: mono(11))),
      if (myCode != null) ...[
        const SizedBox(height: 10),
        note('Send this code to the station\'s owner:'),
        SelectableText(myCode!, style: mono(11, color: bone)),
        Row(children: [pyrateButton('SEND IT', () => ship.invokeMethod('share_text', {'text': 'My Pyrate Radio co-host code: $myCode'}))]),
      ],
      if (msg != null) Padding(padding: const EdgeInsets.only(top: 8), child: note(msg!)),
    ]);
  }
}

/// The station's standing schedule: blocks of the day given to chosen recordings, and the bell.
class RotaEditor extends StatefulWidget {
  final String station;
  final List library;
  const RotaEditor({super.key, required this.station, required this.library});
  @override
  State<RotaEditor> createState() => _RotaEditorState();
}

class _RotaEditorState extends State<RotaEditor> {
  List<Map<String, dynamic>> blocks = [];
  String? ident;
  int every = 60;
  String? msg;

  @override
  void initState() {
    super.initState();
    final r = Native.call({'cmd': 'rota', 'station': widget.station});
    if (r is Map && r['error'] == null) {
      blocks = ((r['blocks'] as List?) ?? []).map((b) => Map<String, dynamic>.from(b as Map)).toList();
      ident = r['ident'] as String?;
      every = (r['ident_every_min'] as num?)?.toInt() ?? 60;
    }
  }

  String hm(int min) => '${(min ~/ 60 % 24).toString().padLeft(2, '0')}:${(min % 60).toString().padLeft(2, '0')}';

  String titleOf(String h) => (widget.library.firstWhere((t) => t['hash'] == h, orElse: () => {'title': 'withdrawn'}) as Map)['title'] ?? '';

  Future<void> addBlock() async {
    final from = await showTimePicker(context: context, initialTime: const TimeOfDay(hour: 8, minute: 0), helpText: 'BLOCK STARTS');
    if (from == null || !mounted) return;
    final to = await showTimePicker(context: context, initialTime: TimeOfDay(hour: (from.hour + 2) % 24, minute: from.minute), helpText: 'BLOCK ENDS');
    if (to == null || !mounted) return;
    final chosen = <String>{};
    var shuffle = true;
    final ok = await showDialog<bool>(
      context: context,
      builder: (c) => StatefulBuilder(
        builder: (c, set) => AlertDialog(
          backgroundColor: panel,
          shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
          title: Text('${hm(from.hour * 60 + from.minute)} TO ${hm(to.hour * 60 + to.minute)}', style: carved(13)),
          content: SizedBox(
            width: double.maxFinite,
            child: ListView(shrinkWrap: true, children: [
              note('Choose what plays in this block. Choose none and the whole library plays.'),
              pyrateSwitch(shuffle, (v) => set(() => shuffle = v), shuffle ? 'Shuffled' : 'In order'),
              for (final t in widget.library)
                CheckboxListTile(
                  dense: true,
                  value: chosen.contains(t['hash']),
                  activeColor: bone,
                  checkColor: black,
                  title: Text(t['title'] ?? '', style: serif(14, color: boneDim)),
                  onChanged: (v) => set(() => v == true ? chosen.add(t['hash']) : chosen.remove(t['hash'])),
                ),
            ]),
          ),
          actions: [TextButton(onPressed: () => Navigator.pop(c, true), child: Text('ADD THE BLOCK', style: carved(11)))],
        ),
      ),
    );
    if (ok != true) return;
    setState(() => blocks.add({
          'start_min': from.hour * 60 + from.minute,
          'end_min': to.hour * 60 + to.minute,
          'tracks': chosen.toList(),
          'shuffle': shuffle,
        }));
  }

  @override
  Widget build(BuildContext context) {
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      heading('THE CLOCK'),
      note('Give hours of the day to chosen recordings, such as talk in the morning and music after dark. Outside those hours everything plays. You can also play one short recording on a regular beat, such as your station\'s name on the hour.'),
      const SizedBox(height: 10),
      for (final (i, b) in blocks.indexed)
        Row(children: [
          SizedBox(width: 110, child: Text('${hm(b['start_min'])} to ${hm(b['end_min'])}', style: mono(12))),
          Expanded(
            child: Text(
              (b['tracks'] as List).isEmpty ? 'the whole library' : (b['tracks'] as List).map((h) => titleOf(h as String)).join(', '),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: serif(14, color: boneDim, italic: true),
            ),
          ),
          IconButton(icon: const Icon(Icons.close, size: 18, color: boneFaint), onPressed: () => setState(() => blocks.removeAt(i))),
        ]),
      Row(children: [pyrateButton('ADD A BLOCK', addBlock)]),
      const SizedBox(height: 14),
      DropdownButton<String?>(
        value: widget.library.any((t) => t['hash'] == ident) ? ident : null,
        dropdownColor: panel,
        isExpanded: true,
        hint: Text('No bell', style: serif(15, color: boneFaint)),
        items: [
          DropdownMenuItem<String?>(value: null, child: Text('No bell', style: serif(15, color: boneDim))),
          for (final t in widget.library) DropdownMenuItem<String?>(value: t['hash'] as String, child: Text('Bell: ${t['title']}', style: serif(15, color: boneDim))),
        ],
        onChanged: (v) => setState(() => ident = v),
      ),
      if (ident != null)
        Row(children: [
          Text('every', style: mono(11)),
          for (final m in [15, 30, 60, 120])
            TextButton(onPressed: () => setState(() => every = m), child: Text('$m MIN', style: carved(11, color: every == m ? bone : boneFaint))),
        ]),
      const SizedBox(height: 8),
      Row(children: [
        pyrateButton('SAVE', () {
          final r = Native.call({'cmd': 'rota_set', 'station': widget.station, 'blocks': blocks, 'ident': ident, 'ident_every_min': every});
          setState(() => msg = r is Map && r['error'] != null ? r['error'] : 'Set. Every listener\'s clock follows it.');
        }),
      ]),
      if (msg != null) Padding(padding: const EdgeInsets.only(top: 8), child: note(msg!)),
    ]);
  }
}

// ---------------------------------------------------------------------------------------
// The ship's papers: settings that belong to this phone alone

/// A setting in plain words: a switch, what it does, and one line more when that helps.
Widget setting(String title, bool value, ValueChanged<bool> onChanged, {String? sub}) => InkWell(
      onTap: () => onChanged(!value),
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 10),
        child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(title, style: serif(17, color: bone)),
              if (sub != null) Padding(padding: const EdgeInsets.only(top: 3), child: Text(sub, style: serif(14, color: boneFaint, italic: true))),
            ]),
          ),
          const SizedBox(width: 12),
          Switch(value: value, onChanged: onChanged, activeThumbColor: bone, activeTrackColor: boneGhost, inactiveThumbColor: boneFaint, inactiveTrackColor: panel),
        ]),
      ),
    );

/// A row that does something when tapped, with a line saying what.
Widget action(String title, VoidCallback onTap, {String? sub, Color color = bone}) => InkWell(
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 12),
        child: Row(children: [
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(title, style: serif(17, color: color)),
              if (sub != null) Padding(padding: const EdgeInsets.only(top: 3), child: Text(sub, style: serif(14, color: boneFaint, italic: true))),
            ]),
          ),
          const Icon(Icons.chevron_right, size: 20, color: boneFaint),
        ]),
      ),
    );

/// A group of settings under a carved heading, set off from the next by a hairline.
Widget section(String title, List<Widget> children) => Padding(
      padding: const EdgeInsets.only(top: 30),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text(title, style: carved(12, color: boneFaint, spacing: 0.3)),
        const SizedBox(height: 4),
        const Divider(height: 1, color: lineSoft),
        ...children,
      ]),
    );

/// Something most people never need, folded away under a plain title.
Widget fold(String title, List<Widget> children, {String? sub}) => Theme(
      data: ThemeData.dark().copyWith(dividerColor: Colors.transparent),
      child: ExpansionTile(
        tilePadding: EdgeInsets.zero,
        childrenPadding: const EdgeInsets.only(top: 8, bottom: 8),
        expandedCrossAxisAlignment: CrossAxisAlignment.start,
        iconColor: boneFaint,
        collapsedIconColor: boneFaint,
        title: Text(title, style: serif(17, color: bone)),
        subtitle: sub == null ? null : Text(sub, style: serif(14, color: boneFaint, italic: true)),
        children: children,
      ),
    );

/// The radio's settings, in the words of someone who has never heard of a relay.
class SettingsPage extends StatefulWidget {
  final Tuner radio;
  const SettingsPage({super.key, required this.radio});
  @override
  State<SettingsPage> createState() => _SettingsPageState();
}

class _SettingsPageState extends State<SettingsPage> {
  bool strict = false;
  bool provisions = true;
  bool plain = false;
  bool plainLock = false;
  bool disguise = false;
  bool pinSet = false;
  bool duressSet = false;
  bool mineOnly = false;
  bool lan = false;
  String tor = '';
  String? gridMsg;
  String? sig;
  Map? canary;
  Map wake = {};
  String? msg;

  @override
  void initState() {
    super.initState();
    strict = (Native.call({'cmd': 'strict'}) as Map?)?['value'] == true;
    provisions = (Native.call({'cmd': 'provisions'}) as Map?)?['value'] != false;
    final pc = Native.call({'cmd': 'pin_check', 'pin': ''}) as Map?;
    pinSet = pc?['set'] == true;
    duressSet = pc?['duress_set'] == true;
    mineOnly = pref('carry_mine_only') == true;
    lan = (Native.call({'cmd': 'lan'}) as Map?)?['value'] == true;
    tor = ((Native.call({'cmd': 'tor'}) as Map?)?['url'] ?? '').toString();
    ship.invokeMethod<bool>('plain_get').then((v) => setState(() => plain = v == true)).catchError((_) {});
    ship.invokeMethod<bool>('plain_lock_get').then((v) => setState(() => plainLock = v == true)).catchError((_) {});
    signer().then((v) { if (mounted) setState(() => sig = v); });
    disguise = ((pref('disguise') as num?) ?? 0) != 0;
    final p = Native.call({'cmd': 'papers'});
    if (p is Map && p['canary'] is Map) canary = p['canary'] as Map;
    ship.invokeMethod<String>('wake_get').then((v) => setState(() => wake = jsonDecode(v ?? '{}'))).catchError((_) {});
  }

  Future<void> setWake() async {
    final t = await showTimePicker(
      context: context,
      initialTime: TimeOfDay(hour: (wake['hour'] as num?)?.toInt() ?? 7, minute: (wake['minute'] as num?)?.toInt() ?? 0),
      helpText: 'WAKE TO THE RADIO AT',
    );
    if (t == null) return;
    final st = widget.radio.tuned ?? (widget.radio.stations.isNotEmpty ? widget.radio.stations.first['id'] as String : null);
    if (st == null) return;
    final name = widget.radio.station(st)?['name'] ?? '';
    await ship.invokeMethod('wake_set', {'on': true, 'hour': t.hour, 'minute': t.minute, 'station': st, 'name': name});
    final exact = await ship.invokeMethod<bool>('exact_ok').catchError((_) => true) ?? true;
    final v = await ship.invokeMethod<String>('wake_get');
    if (mounted) setState(() {
      wake = jsonDecode(v ?? '{}');
      msg = exact ? null : 'Android asks permission for alarms that ring on the minute. Without it the radio may wake you a minute late.';
    });
    if (!exact) await ship.invokeMethod('exact_settings');
  }

  Future<void> setPin({bool duress = false}) async {
    final pin = TextEditingController();
    final ok = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        backgroundColor: panel,
        shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
        title: Text(duress ? 'An emergency PIN' : 'Choose a PIN', style: serif(22)),
        content: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          note(duress
              ? 'If someone forces you to unlock the radio, give them this one. It wipes the radio from this phone and closes. Your 24 words bring your station back later.'
              : 'Asked for before your station and these settings open.'),
          const SizedBox(height: 10),
          TextField(controller: pin, autofocus: true, keyboardType: TextInputType.number, obscureText: true, enableSuggestions: false, autocorrect: false, enableIMEPersonalizedLearning: false, style: mono(16, color: bone), decoration: pyrateField('Four or more digits, not your phone\'s PIN')),
        ]),
        actions: [TextButton(onPressed: () => Navigator.pop(c, true), child: Text('SET IT', style: carved(11)))],
      ),
    );
    if (ok != true || pin.text.length < 4) return;
    await Native.callAway({'cmd': 'pin_set', 'pin': pin.text, 'duress': duress});
    pin.clear();
    if (!mounted) return;
    setState(() => duress ? duressSet = true : pinSet = true);
  }

  Future<void> importList() async {
    final text = TextEditingController();
    final ok = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        backgroundColor: panel,
        shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
        title: Text('Add a hidden-stations list', style: serif(22)),
        content: SingleChildScrollView(
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            note('Paste a list someone shared. Every station on it is hidden from your radio and from Stations.'),
            TextField(enableIMEPersonalizedLearning: false, controller: text, maxLines: 6, style: mono(11, color: bone), decoration: pyrateField('The list')),
          ]),
        ),
        actions: [
          TextButton(
            onPressed: () async {
              final picked = await FilePicker.platform.pickFiles(type: FileType.any);
              final p = picked?.files.single.path;
              if (p != null) text.text = await File(p).readAsString();
            },
            child: Text('FROM A FILE', style: carved(11, color: boneFaint)),
          ),
          TextButton(onPressed: () => Navigator.pop(c, true), child: Text('HIDE THEM', style: carved(11))),
        ],
      ),
    );
    if (ok != true) return;
    final r = await Native.callAway({'cmd': 'maroon_import', 'text': text.text});
    if (!mounted) return;
    setState(() => msg = r is Map && r['added'] != null ? '${r['added']} stations hidden.' : 'That list would not read.');
  }

  Future<void> scuttle() async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        title: Text('Wipe the radio from this phone?', style: serif(22)),
        content: Text('Everything it keeps here becomes unreadable at once and cannot be brought back. Your station stays on the air, and your 24 words bring it to any phone.', style: serif(15, color: boneFaint, italic: true)),
        actions: [
          TextButton(onPressed: () => Navigator.pop(c, false), child: Text('NOT NOW', style: carved(11, color: boneFaint))),
          TextButton(onPressed: () => Navigator.pop(c, true), child: Text('WIPE IT', style: carved(11))),
        ],
      ),
    );
    if (ok == true) await ship.invokeMethod('scuttle').catchError((_) {});
  }

  @override
  Widget build(BuildContext context) {
    final wakeOn = wake['on'] == true;
    final all = Native.call({'cmd': 'stations', 'marooned': true});
    final hidden = all is List ? all.where((x) => x['marooned'] == true).toList() : [];
    final carried = widget.radio.carriedBytes;
    return ListView(padding: const EdgeInsets.fromLTRB(24, 26, 24, 40), children: [
      masthead('Settings'),
      section('LISTENING', [
        setting('A soft hiss while tuning in', pref('hiss') != false, (v) {
          Native.call({'cmd': 'hiss', 'value': v});
          setState(() => pref('hiss', v));
        }, sub: 'So you hear the radio working before the station comes in.'),
        setting('Hide mature stations', pref('hide_mature') == true, (v) => setState(() => pref('hide_mature', v)), sub: 'Strong language or themes, as each station says of itself.'),
        setting('Show explicit stations', pref('show_explicit') == true, (v) => setState(() => pref('show_explicit', v)), sub: 'Sexual or graphic material. Hidden until you turn this on.'),
        setting('Pictures only from stations I keep', (Native.call({'cmd': 'art_kept_only'}) as Map)['value'] == true, (v) => setState(() => Native.call({'cmd': 'art_kept_only', 'value': v})), sub: 'Other stations show by name alone.'),
        setting('Save ahead on Wi-Fi', provisions, (v) {
          Native.call({'cmd': 'provisions', 'value': v});
          setState(() => provisions = v);
        }, sub: 'The next few hours of your kept stations, ready for when you have no signal.'),
        if (Platform.isAndroid)
          action(wakeOn ? 'Wake to the radio at ${(wake['hour'] as num).toInt().toString().padLeft(2, '0')}:${(wake['minute'] as num).toInt().toString().padLeft(2, '0')}' : 'Wake to the radio', setWake,
              sub: wakeOn ? '${wake['name'] ?? 'The station'} rises from quiet each morning. Tap to change.' : 'A station instead of an alarm. If no sound comes, the phone\'s alarm rings.'),
        if (Platform.isAndroid && wakeOn)
          action('Turn the wake-up off', () async {
            await ship.invokeMethod('wake_set', {'on': false, 'hour': 7, 'minute': 0});
            if (mounted) setState(() => wake = {'on': false});
          }, color: boneDim),
        fold('Hidden stations', sub: hidden.isEmpty ? 'None. To hide a station, tap the three dots beside it in Stations.' : '${hidden.length} hidden', [
          for (final m in hidden)
            Row(children: [
              Expanded(child: Text(m['name'] ?? '', style: serif(16, color: boneDim))),
              TextButton(
                onPressed: () async {
                  await Native.callAway({'cmd': 'maroon', 'station': m['id'], 'value': false});
                  if (mounted) setState(() {});
                },
                child: Text('SHOW AGAIN', style: carved(11, color: boneFaint)),
              ),
            ]),
          const SizedBox(height: 6),
          Wrap(spacing: 10, runSpacing: 10, children: [
            pyrateButton('SHARE MY LIST', () {
              final r = Native.call({'cmd': 'maroon_export'});
              if (r is Map) ship.invokeMethod('share_text', {'text': r['text']});
            }),
            pyrateButton('ADD A LIST', importList),
          ]),
        ]),
        if (msg != null) Padding(padding: const EdgeInsets.only(top: 6), child: note(msg!)),
      ]),
      section('KEEPING STATIONS ON THE AIR', [
        const SizedBox(height: 10),
        note('Pyrate Radio has no servers. Every device that runs it keeps a small, sealed share of the stations so they stay on the air.${carried >= 100000000 ? ' This one is carrying ${(carried / 1e9).toStringAsFixed(1)} GB.' : ''}'),
        setting('Help carry stations', !mineOnly, (v) {
          pref('carry_mine_only', !v);
          if (!v) Native.call({'cmd': 'ballast_ok', 'value': false, 'unmetered': false});
          setState(() => mineOnly = !v);
        }, sub: Platform.isAndroid ? 'Only on Wi-Fi while the phone charges, so it costs no data or battery.' : 'Uses up to 20 GB, less on a fuller disk.'),
        if (Platform.isAndroid) const DrawerLantern(),
      ]),
      section('PRIVACY AND SAFETY', [
        setting('Lock with a PIN', pinSet, (v) {
          if (v) {
            setPin();
          } else {
            Native.call({'cmd': 'pin_set'});
            setState(() => pinSet = duressSet = false);
          }
        }, sub: 'Asked for before your station and these settings open.'),
        if (pinSet)
          setting('Emergency PIN', duressSet, (v) {
            if (v) {
              setPin(duress: true);
            } else {
              Native.call({'cmd': 'pin_set', 'duress': true});
              setState(() => duressSet = false);
            }
          }, sub: 'A second PIN to give if you are forced to unlock the radio. It wipes the radio from this phone.'),
        setting('Disguise my voice', disguise, (v) {
          pref('disguise', v ? -4 : 0);
          setState(() => disguise = v);
        }, sub: 'Lower and harder to recognise, when you go live or record.'),
        if (Platform.isAndroid) ...[
          setting('Plain icon', plain, (v) {
            ship.invokeMethod('plain_set', {'value': v});
            setState(() => plain = v);
          }, sub: 'Shows as "Radio" with an ordinary icon.'),
          setting('Hide from the lock screen', plainLock, (v) {
            ship.invokeMethod('plain_lock_set', {'value': v});
            setState(() => plainLock = v);
          }, sub: 'The lock screen says only "Radio", and recent apps show nothing.'),
        ],
        action('Clear what I have listened to', () async {
          final r = await Native.callAway({'cmd': 'clear_hold'});
          if (!context.mounted) return;
          final n = r is Map ? (r['cleared'] as num?)?.toInt() : null;
          toast(context, r is Map && r['error'] != null ? 'Not cleared: ${r['error']}' : (n == null || n == 0 ? 'Nothing to clear.' : 'Cleared.'));
        }, sub: 'Removes the sound saved from stations you played. Your own station is kept.'),
        action('Wipe the radio from this phone', scuttle, sub: 'For a phone about to be taken. Everything it keeps becomes unreadable at once.', color: bloodBright),
      ]),
      section('SHARING', [
        action('Send the app to someone nearby', () => pyrateSheet(context, const PassItOn()), sub: 'Phone to phone, no internet needed.'),
        action('Open a station file', () async {
          final picked = await FilePicker.platform.pickFiles();
          final path = picked?.files.single.path;
          if (path == null) return;
          final r = await Native.callAway({'cmd': 'pack_import', 'path': path});
          if (!mounted) return;
          if (r is Map && r['station'] is String) {
            await Native.callAway({'cmd': 'keep', 'station': r['station'], 'value': true});
            if (context.mounted) toast(context, 'The station is on your dial.');
          } else {
            toast(context, 'That file would not open: ${r is Map ? r['error'] : 'not a station file'}.');
          }
        }, sub: 'A station someone sent you as a file plays with no connection.'),
      ]),
      section('ADVANCED', [
        fold('Connections', sub: 'For when the internet is watched or cut', [
          setting('Go through Tor', tor.isNotEmpty, (v) async {
            final r = await Native.callAway({'cmd': 'tor', 'url': v ? 'http://127.0.0.1:8118' : ''});
            if (!mounted) return;
            setState(() {
              tor = (r is Map ? r['url'] ?? '' : '').toString();
              gridMsg = r is Map && r['error'] != null
                  ? 'Not set: ${r['error']}.'
                  : v
                      ? 'From the next time the radio starts, every connection goes through Tor.'
                      : 'From the next time the radio starts, it connects as usual.';
            });
          }, sub: '${Platform.isAndroid ? 'Needs the Orbot app running.' : 'Needs Tor running on this computer, with its HTTP port on 8118.'} Nobody on the network learns this device\'s address.'),
          setting('Find radios on this Wi-Fi', lan, (v) {
            final r = Native.call({'cmd': 'lan', 'value': v});
            setState(() {
              lan = (r as Map?)?['value'] == true;
              gridMsg = v && r is Map && r['dark'] == true ? 'This stays off while the radio goes through Tor.' : null;
            });
          }, sub: 'Radios nearby find each other with no internet. Anyone on the same Wi-Fi can tell a radio is here.'),
          setting('Connect directly when needed', !strict, (v) {
            Native.call({'cmd': 'direct_ok', 'value': v});
            setState(() => strict = !v);
          }, sub: 'Off, the radio always goes through two other devices and waits when it cannot. On, it plays sooner on a quiet network, and the device it asks sees this phone\'s address.'),
          if (gridMsg != null) Padding(padding: const EdgeInsets.only(top: 6), child: note(gridMsg!)),
        ]),
        fold('About this copy', [
          if (sig != null) ...[
            note('Signed by HOSTIS with the key below. hostis.black/pyrateradio shows the same fingerprint, and a copy signed with any other is not ours.'),
            const SizedBox(height: 6),
            SelectableText(sig!, style: mono(11, color: boneDim)),
          ],
          if (canary != null) ...[
            const SizedBox(height: 14),
            note((canary!['age_days'] as num) > 45
                ? 'HOSTIS\'s monthly statement is ${canary!['age_days']} days old. A statement left unrenewed may mean HOSTIS can no longer say what it would.'
                : 'HOSTIS\'s monthly statement, signed ${shortDate((canary!['at_ms'] as num).toInt())}: ${canary!['text']}'),
          ],
        ]),
      ]),
    ]);
  }
}

/// Asks for the PIN before your station or the settings open, once each time the app comes
/// to the front.
class PinGate extends StatefulWidget {
  final Widget child;
  const PinGate({super.key, required this.child});
  @override
  State<PinGate> createState() => _PinGateState();
}

class _PinGateState extends State<PinGate> with WidgetsBindingObserver {
  /// One unlock opens both your station and the settings until the app goes to the back.
  static final unlocked = ValueNotifier(false);
  bool get open => unlocked.value;
  set open(bool v) => unlocked.value = v;
  final pin = TextEditingController();
  String? wrong;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    unlocked.addListener(_changed);
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    unlocked.removeListener(_changed);
    pin.dispose();
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.paused) setState(() => open = false);
  }

  @override
  Widget build(BuildContext context) {
    final set = (Native.call({'cmd': 'pin_check', 'pin': ''}) as Map?)?['set'] == true;
    if (!set || open) return widget.child;
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(40),
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          Text('Locked', style: pica(24, spacing: 0.06)),
          const SizedBox(height: 14),
          TextField(
            controller: pin,
            autofocus: true,
            obscureText: true,
            enableSuggestions: false,
            autocorrect: false,
            enableIMEPersonalizedLearning: false,
            keyboardType: TextInputType.number,
            textAlign: TextAlign.center,
            style: mono(20, color: bone),
            decoration: pyrateField('PIN'),
            onSubmitted: (_) => check(),
          ),
          if (wrong != null) Padding(padding: const EdgeInsets.only(top: 8), child: Text(wrong!, style: serif(14, color: bone, italic: true))),
          const SizedBox(height: 14),
          pyrateButton('OPEN', check),
        ]),
      ),
    );
  }

  Future<void> check() async {
    // The PIN is kept under a deliberately slow hash: worked out off the screen's thread.
    final r = await Native.callAway({'cmd': 'pin_check', 'pin': pin.text}) as Map?;
    if (r?['duress'] == true) {
      // The PIN given up under force: everything the radio keeps becomes noise, and it closes.
      await ship.invokeMethod('scuttle').catchError((_) {});
      return;
    }
    final ok = r?['ok'] == true;
    if (!mounted) return;
    setState(() {
      open = ok;
      wrong = ok ? null : 'Not that one.';
      pin.clear();
    });
  }
}
