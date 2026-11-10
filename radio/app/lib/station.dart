// Pyrate Radio: a station's own page, its programme, its shows and notices, and the
// shapes a new station can start in. No rights reserved.

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:path_provider/path_provider.dart';

import 'art.dart';
import 'main.dart';
import 'more.dart';
import 'native.dart';

// ---------------------------------------------------------------------------------------
// Local time, as the station keeps it

/// This phone's offsets from UTC for the next forty years, daylight saving included, so a
/// station keeps local time long after its keeper has stopped looking.
List<List<int>> zoneOffsets() {
  const day = 86400000;
  int off(int ms) => DateTime.fromMillisecondsSinceEpoch(ms).timeZoneOffset.inMinutes;
  var t = DateTime.now().millisecondsSinceEpoch - 400 * day;
  final end = DateTime.now().millisecondsSinceEpoch + 40 * 365 * day;
  var cur = off(t);
  final out = <List<int>>[[0, cur]];
  while (t < end) {
    final n = t + day;
    final o = off(n);
    if (o != cur) {
      var lo = t, hi = n;
      while (hi - lo > 60000) {
        final mid = lo + (hi - lo) ~/ 2;
        if (off(mid) == o) {
          hi = mid;
        } else {
          lo = mid;
        }
      }
      out.add([hi ~/ 60000 * 60000, o]);
      cur = o;
    }
    t = n;
  }
  return out;
}

String zoneLabel() {
  final o = DateTime.now().timeZoneOffset;
  final sign = o.isNegative ? '−' : '+';
  final h = o.inHours.abs().toString().padLeft(2, '0');
  final m = (o.inMinutes.abs() % 60).toString().padLeft(2, '0');
  return '${DateTime.now().timeZoneName} · UTC$sign$h:$m';
}

String hhmm(int min) => '${(min ~/ 60 % 24).toString().padLeft(2, '0')}:${(min % 60).toString().padLeft(2, '0')}';
const weekDays = ['MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT', 'SUN'];
const weekDayNames = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];

String daysLabel(int mask) {
  if (mask & 0x7f == 0x7f) return 'every day';
  if (mask == 0x1f) return 'weekdays';
  if (mask == 0x60) return 'weekends';
  return [for (var d = 0; d < 7; d++) if (mask & (1 << d) != 0) weekDayNames[d].substring(0, 3)].join(', ');
}

String lengthLabel(int min) => min % 60 == 0 ? '${min ~/ 60} hour${min == 60 ? '' : 's'}' : min < 60 ? '$min minutes' : '${min ~/ 60}h ${min % 60}m';

/// One line on how a station plays, from its programme.
String programmeSummary(Map? p) {
  if (p == null) return 'Plays its whole library shuffled, round the clock.';
  final zone = (p['zone_name'] ?? '').toString();
  final at = zone.isEmpty ? '' : ' $zone';
  final m = (p['mode'] as Map?) ?? {};
  final start = (m['start_min'] as num?)?.toInt() ?? 0;
  var s = switch (m['kind']) {
    'in_order' => 'Plays its library in order, from the top each Monday',
    'newest_first' => 'Plays its newest recordings first',
    'daily' => 'Plays its library in order, from the top every day at ${hhmm(start)}$at',
    'weekly' => 'Plays its library in order, from the top every ${weekDayNames[((m['day'] as num?)?.toInt() ?? 0) % 7]} at ${hhmm(start)}$at',
    _ => 'Plays its whole library shuffled',
  };
  final hours = (p['hours'] as List?) ?? [];
  if (hours.isEmpty) {
    s += ', round the clock.';
  } else {
    s += ', on the air ${hours.map((h) => '${daysLabel((h['days'] as num).toInt())} from ${hhmm((h['start_min'] as num).toInt())} for ${lengthLabel((h['len_min'] as num).toInt())}').join(' and ')}$at.';
  }
  final slots = (p['slots'] as List?) ?? [];
  if (slots.isNotEmpty) s += ' ${slots.length} regular ${slots.length == 1 ? 'show' : 'shows'}.';
  return s;
}

// ---------------------------------------------------------------------------------------
// The station's page

void openPage(BuildContext context, Map s, Tuner radio) =>
    Navigator.push(context, MaterialPageRoute(builder: (_) => StationPage(station: Map<String, dynamic>.from(s), radio: radio)));

/// "today", "tomorrow" or "on Friday".
String whenLabel(int ms) {
  final d = dayLabel(ms);
  return switch (d) { 'TODAY' => 'today', 'TOMORROW' => 'tomorrow', _ => 'on ${d[0]}${d.substring(1).toLowerCase()}' };
}

String offAirLabel(int? back) => back == null ? 'Off the air' : 'Off the air. Back ${whenLabel(back)} at ${clockTime(back)}.';

class StationPage extends StatefulWidget {
  final Map<String, dynamic> station;
  final Tuner radio;
  const StationPage({super.key, required this.station, required this.radio});
  @override
  State<StationPage> createState() => _StationPageState();
}

class _StationPageState extends State<StationPage> {
  Map? page;
  String? error;
  Set<int> bells = {};
  bool week = false;

  @override
  void initState() {
    super.initState();
    load();
  }

  Future<void> load() async {
    final r = await Native.callAway({'cmd': 'page', 'station': widget.station['id'], 'days': 7});
    final b = await ship.invokeMethod<String>('bells').catchError((_) => '[]') ?? '[]';
    if (!mounted) return;
    setState(() {
      if (r is Map && r['error'] == null) {
        page = r;
        error = null;
      } else {
        error = (r is Map ? r['error'] : null) ?? 'This station\'s log has not arrived yet.';
      }
      bells = (jsonDecode(b) as List).map((x) => (x['id'] as num).toInt()).toSet();
    });
  }

  List listingsOf(Map p) => (p['listings'] as List? ?? []).where((l) => l['kind'] != 'hours').toList();

  /// Today and tomorrow, or the whole week once asked for.
  List shownListings(Map p) {
    final all = listingsOf(p);
    if (week) return all;
    final days = <String>{};
    return all.where((l) => (days..add(dayLabel((l['start_ms'] as num).toInt()))).length <= 2).toList();
  }

  Widget section(String title) => Padding(padding: const EdgeInsets.only(top: 26, bottom: 8), child: Text(title, style: carved(11, color: bloodBright, spacing: 0.45)));

  @override
  Widget build(BuildContext context) {
    final p = page;
    // The log's own word on the station, where it has arrived; the Log's card until then.
    final s = {...widget.station, if (p != null) for (final k in ['name', 'about', 'port', 'tip']) if ((p[k] ?? '').toString().isNotEmpty) k: p[k]};
    final id = s['id'] as String;
    final own = p?['own'] == true;
    final now = DateTime.now().millisecondsSinceEpoch;
    String? lastDay;
    return Scaffold(
      backgroundColor: black,
      appBar: AppBar(backgroundColor: black, elevation: 0, title: Text((s['call_sign'] ?? '').toString(), style: mono(12, color: boneFaint))),
      body: RefreshIndicator(
        onRefresh: load,
        child: ListView(padding: const EdgeInsets.fromLTRB(24, 0, 24, 48), children: [
          if (s['banner'] != null)
            StationArt(station: id, hash: s['banner'], width: double.infinity, height: 150, fallback: const SizedBox()),
          const SizedBox(height: 18),
          Row(children: [
            s['cover'] == null
                ? Flag(id: id, width: 64, live: s['live'] == true)
                : StationArt(station: id, hash: s['cover'], width: 72, height: 72, fallback: Flag(id: id, width: 64, live: false)),
            const SizedBox(width: 16),
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Row(children: [Flexible(child: Text(s['name'] ?? '', style: serif(26))), ...seals(s, size: 9)]),
                if ((s['port'] ?? '').toString().isNotEmpty) Text((s['port']).toString().toUpperCase(), style: carved(9, color: boneFaint, spacing: 0.25)),
                if (((s['kinds'] as List?) ?? []).isNotEmpty) Text(((s['kinds'] as List).join(' · ')).toUpperCase(), style: carved(9, color: boneGhost, spacing: 0.25)),
              ]),
            ),
          ]),
          if ((s['about'] ?? '').toString().isNotEmpty) Padding(padding: const EdgeInsets.only(top: 14), child: Text(s['about'], style: serif(16, color: boneDim, italic: true))),
          const SizedBox(height: 14),
          Wrap(spacing: 4, runSpacing: 4, children: [
            pyrateButton('▶ LISTEN', () {
              widget.radio.tune(id);
              Navigator.pop(context);
            }),
            pyrateButton('PASS IT ON', () => shareStation(s)),
            if ((s['tip'] ?? '').toString().isNotEmpty) pyrateButton('TIP', () => tipStation(context, s['tip'])),
            pyrateButton('A POSTER', () => pyrateSheet(context, PosterSheet(station: s))),
          ]),
          if (own) ...[
            section('YOUR STATION'),
            Wrap(spacing: 4, runSpacing: 4, children: [
              pyrateButton('POST A NOTICE', () async {
                await pyrateSheet(context, NoticeEditor(station: id));
                load();
              }),
              pyrateButton('A PREMIERE', () async {
                await pyrateSheet(context, ShowEditor(station: id));
                load();
              }),
              pyrateButton('PROGRAMME', () async {
                await Navigator.push(context, MaterialPageRoute(builder: (_) => Scaffold(backgroundColor: black, appBar: AppBar(backgroundColor: black), body: ProgrammeEditor(station: id))));
                load();
              }),
            ]),
          ],
          if (error != null) Padding(padding: const EdgeInsets.only(top: 24), child: note(error!)),
          if (p == null && error == null) Padding(padding: const EdgeInsets.only(top: 24), child: note('Reading the station\'s log')),
          if (p != null) ...[
            if (((p['notices'] as List?) ?? []).isNotEmpty) ...[
              section('NOTICES'),
              for (final n in p['notices'] as List)
                Container(
                  margin: const EdgeInsets.only(bottom: 8),
                  padding: const EdgeInsets.all(14),
                  decoration: BoxDecoration(border: Border.all(color: boneGhost, width: 0.5)),
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text(n['text'] ?? '', style: serif(16, color: bone)),
                    const SizedBox(height: 6),
                    Row(children: [
                      Text('${dayLabel((n['at_ms'] as num).toInt())} ${clockTime((n['at_ms'] as num).toInt())}', style: mono(10, color: boneGhost)),
                      const Spacer(),
                      if (own)
                        TextButton(
                          onPressed: () async {
                            await Native.callAway({'cmd': 'notice_remove', 'station': id, 'notice': n['id']});
                            load();
                          },
                          child: Text('TAKE DOWN', style: carved(9, color: boneFaint)),
                        ),
                    ]),
                  ]),
                ),
            ],
            section('ON NOW'),
            if (p['now'] != null) ...[
              if ((p['now']['slot'] ?? '').toString().isNotEmpty) Text((p['now']['slot']).toString().toUpperCase(), style: carved(10, color: bloodBright, spacing: 0.3)),
              Text(p['now']['title'] ?? '', style: serif(18, color: boneDim, italic: true)),
              if ((p['now']['by'] ?? '').toString().isNotEmpty) Text(p['now']['by'], style: carved(10, color: boneFaint, spacing: 0.2)),
            ] else
              note(p['back_at'] != null
                  ? offAirLabel((p['back_at'] as num).toInt())
                  : ((p['tracks'] as num?) ?? 0) == 0
                      ? 'Nothing on the clock yet.'
                      : 'Off the air.'),
            if (((p['next'] as List?) ?? []).length > 1) ...[
              section('UP NEXT'),
              for (final n in (p['next'] as List).skip(1).take(5))
                Padding(
                  padding: const EdgeInsets.only(bottom: 4),
                  child: Row(children: [
                    SizedBox(width: 54, child: Text(clockTime((n['start_ms'] as num).toInt()), style: mono(12, color: boneFaint))),
                    Expanded(child: Text(n['title'] ?? '', maxLines: 1, overflow: TextOverflow.ellipsis, style: serif(15, color: boneDim))),
                  ]),
                ),
            ],
            section('CHATTER'),
            ChatterPanel(
              station: id,
              own: own,
              nextAt: ((p['next'] as List?) ?? []).map((n) => (n['start_ms'] as num).toInt()).where((t) => t > now).fold<int?>(null, (a, b) => a ?? b),
            ),
            section('THE SCHEDULE'),
            note(programmeSummary(p['programme'] as Map?)),
            if (((p['listings'] as List?) ?? []).isEmpty)
              Padding(padding: const EdgeInsets.only(top: 8), child: note(own ? 'No shows yet. Add a regular show in the programme, or a premiere.' : 'No shows on the bill this week.')),
            for (final l in shownListings(p)) ...[
              if (dayLabel((l['start_ms'] as num).toInt()) != lastDay)
                Builder(builder: (_) {
                  lastDay = dayLabel((l['start_ms'] as num).toInt());
                  return Padding(padding: const EdgeInsets.only(top: 14, bottom: 4), child: Text(lastDay!, style: carved(10, color: boneFaint, spacing: 0.3)));
                }),
              Builder(builder: (_) {
                final start = (l['start_ms'] as num).toInt();
                final end = (l['end_ms'] as num).toInt();
                final onNow = start <= now && now < end;
                final id2 = bellId('${l['title']}${l['kind']}', start);
                final ringing = bells.contains(id2);
                return Padding(
                  padding: const EdgeInsets.only(bottom: 6),
                  child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    SizedBox(width: 92, child: Text('${clockTime(start)}–${clockTime(end)}', style: mono(11, color: onNow ? bone : boneFaint, weight: onNow ? FontWeight.w700 : FontWeight.w400))),
                    Expanded(
                      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                        Row(children: [
                          Flexible(child: Text(l['title'] ?? '', style: serif(16, color: onNow ? bone : boneDim))),
                          if (l['live'] == true) Padding(padding: const EdgeInsets.only(left: 8), child: Text('LIVE', style: carved(8.5, color: bloodBright, spacing: 0.3))),
                          if (l['kind'] == 'show') Padding(padding: const EdgeInsets.only(left: 8), child: Text('PREMIERE', style: carved(8.5, color: bone, spacing: 0.3))),
                        ]),
                        if ((l['about'] ?? '').toString().isNotEmpty) Text(l['about'], style: serif(13, color: boneFaint, italic: true)),
                        if (own && l['show'] != null)
                          TextButton(
                            style: TextButton.styleFrom(padding: EdgeInsets.zero, minimumSize: const Size(0, 26)),
                            onPressed: () async {
                              await Native.callAway({'cmd': 'show_cancel', 'station': id, 'show': l['show']});
                              load();
                            },
                            child: Text('CALL IT OFF', style: carved(8.5, color: boneFaint)),
                          ),
                      ]),
                    ),
                    if (!onNow && start > now)
                      IconButton(
                        visualDensity: VisualDensity.compact,
                        tooltip: ringing ? 'Silence the bell' : 'Ring a bell when it starts',
                        icon: Icon(ringing ? Icons.notifications_active : Icons.notifications_none, size: 18, color: ringing ? brass : boneGhost),
                        onPressed: () async {
                          if (ringing) {
                            await ship.invokeMethod('bell_cancel', {'id': id2});
                            setState(() => bells.remove(id2));
                          } else {
                            await ship.invokeMethod('bell_set', {'id': id2, 'at': start, 'title': l['title'] ?? 'A show', 'station': id, 'name': s['name']});
                            setState(() => bells.add(id2));
                          }
                        },
                      ),
                  ]),
                );
              }),
            ],
            if (!week && shownListings(p).length < listingsOf(p).length)
              Padding(padding: const EdgeInsets.only(top: 8), child: Row(children: [pyrateButton('THE WHOLE WEEK', () => setState(() => week = true))])),
            const SizedBox(height: 18),
            note('Worked out on this phone from the station\'s own log. Times are yours.'),
          ],
        ]),
      ),
    );
  }
}

// ---------------------------------------------------------------------------------------
// Notices and premieres

class NoticeEditor extends StatefulWidget {
  final String station;
  const NoticeEditor({super.key, required this.station});
  @override
  State<NoticeEditor> createState() => _NoticeEditorState();
}

class _NoticeEditorState extends State<NoticeEditor> {
  final text = TextEditingController();
  int days = 0;
  String? err;

  @override
  Widget build(BuildContext context) => Padding(
        padding: EdgeInsets.fromLTRB(24, 22, 24, 24 + MediaQuery.of(context).viewInsets.bottom),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          heading('A NOTICE'),
          note('A word to everyone who opens your station\'s page. It travels in the log like everything else.'),
          TextField(controller: text, maxLines: 4, maxLength: 600, style: serif(17), decoration: pyrateField('What you want to say')),
          Wrap(children: [
            for (final (d, label) in [(0, 'Until I take it down'), (1, 'A day'), (7, 'A week')]) chip(label, days == d, () => setState(() => days = d)),
          ]),
          const SizedBox(height: 12),
          pyrateButton('POST IT', () async {
            final until = days == 0 ? 0 : DateTime.now().millisecondsSinceEpoch + days * 86400000;
            final r = await Native.callAway({'cmd': 'notice_post', 'station': widget.station, 'text': text.text, 'until_ms': until});
            if (r is Map && r['error'] != null) {
              setState(() => err = r['error']);
            } else if (context.mounted) {
              Navigator.pop(context);
            }
          }),
          if (err != null) Padding(padding: const EdgeInsets.only(top: 8), child: Text(err!, style: serif(14, color: bloodBright))),
        ]),
      );
}

/// Pick recordings from the station's library, in the order tapped.
class TrackPicker extends StatelessWidget {
  final List library;
  final List<String> picked;
  final ValueChanged<List<String>> onChanged;
  const TrackPicker({super.key, required this.library, required this.picked, required this.onChanged});

  @override
  Widget build(BuildContext context) => Column(children: [
        for (final t in library)
          InkWell(
            onTap: () {
              final h = t['track'] as String;
              final next = [...picked];
              next.contains(h) ? next.remove(h) : next.add(h);
              onChanged(next);
            },
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: 6),
              child: Row(children: [
                SizedBox(
                  width: 28,
                  child: picked.contains(t['track']) ? Text('${picked.indexOf(t['track']) + 1}', style: mono(12, color: bloodBright, weight: FontWeight.w700)) : const Text('·', style: TextStyle(color: boneGhost)),
                ),
                Expanded(child: Text(t['title'] ?? '', maxLines: 1, overflow: TextOverflow.ellipsis, style: serif(15, color: picked.contains(t['track']) ? bone : boneFaint))),
              ]),
            ),
          ),
      ]);
}

class ShowEditor extends StatefulWidget {
  final String station;
  const ShowEditor({super.key, required this.station});
  @override
  State<ShowEditor> createState() => _ShowEditorState();
}

class _ShowEditorState extends State<ShowEditor> {
  final title = TextEditingController();
  final about = TextEditingController();
  DateTime when = DateTime.now().add(const Duration(hours: 1));
  List library = [];
  List<String> picked = [];
  bool live = false;
  int liveMin = 60;
  String? err;

  @override
  void initState() {
    super.initState();
    Native.callAway({'cmd': 'programme', 'station': widget.station}).then((r) {
      if (mounted && r is Map) setState(() => library = (r['library'] as List?) ?? []);
    });
  }

  @override
  Widget build(BuildContext context) => Padding(
        padding: EdgeInsets.fromLTRB(24, 22, 24, 24 + MediaQuery.of(context).viewInsets.bottom),
        child: ListView(shrinkWrap: true, children: [
          heading('A PREMIERE'),
          note('A one-off airing at a set minute. Everyone aboard hears it at the same second, and anyone who kept the station can ring a bell for it.'),
          TextField(controller: title, maxLength: 80, style: serif(18), decoration: pyrateField('What it is called')),
          TextField(controller: about, maxLength: 400, style: serif(15), decoration: pyrateField('A line about it')),
          Row(children: [
            Text('${dayLabel(when.millisecondsSinceEpoch)} ${clockTime(when.millisecondsSinceEpoch)}', style: mono(14, color: bone)),
            const Spacer(),
            pyrateButton('WHEN', () async {
              final d = await showDatePicker(context: context, initialDate: when, firstDate: DateTime.now(), lastDate: DateTime.now().add(const Duration(days: 365)));
              if (d == null || !context.mounted) return;
              final t = await showTimePicker(context: context, initialTime: TimeOfDay.fromDateTime(when));
              if (t == null) return;
              setState(() => when = DateTime(d.year, d.month, d.day, t.hour, t.minute));
            }),
          ]),
          const SizedBox(height: 14),
          Text('THE RECORDINGS, IN ORDER', style: carved(10, color: boneFaint, spacing: 0.25)),
          if (library.isEmpty) note('Add recordings to the station first, or make it a live hour.'),
          TrackPicker(library: library, picked: picked, onChanged: (v) => setState(() => picked = v)),
          pyrateSwitch(live, (v) => setState(() => live = v), 'Then I go live'),
          if (live)
            Wrap(children: [for (final m in [30, 60, 90, 120]) chip(lengthLabel(m), liveMin == m, () => setState(() => liveMin = m))]),
          const SizedBox(height: 12),
          pyrateButton('SET IT', () async {
            final r = await Native.callAway({
              'cmd': 'show_add', 'station': widget.station, 'title': title.text.trim().isEmpty ? 'A premiere' : title.text.trim(), 'about': about.text.trim(),
              'at_ms': when.millisecondsSinceEpoch, 'tracks': picked, 'live_min': live ? liveMin : 0,
            });
            if (r is Map && r['error'] != null) {
              setState(() => err = r['error']);
            } else if (context.mounted) {
              Navigator.pop(context);
            }
          }),
          if (err != null) Padding(padding: const EdgeInsets.only(top: 8), child: Text(err!, style: serif(14, color: bloodBright))),
        ]),
      );
}

Widget chip(String label, bool on, VoidCallback onTap) => Padding(
      padding: const EdgeInsets.only(right: 8, bottom: 8),
      child: InkWell(
        onTap: onTap,
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
          decoration: BoxDecoration(border: Border.all(color: on ? bone : boneGhost, width: on ? 0.9 : 0.5)),
          child: Text(label.toUpperCase(), style: carved(9.5, color: on ? bone : boneFaint, spacing: 0.2)),
        ),
      ),
    );

Future<int?> pickTime(BuildContext context, int min) async {
  final t = await showTimePicker(context: context, initialTime: TimeOfDay(hour: min ~/ 60, minute: min % 60));
  return t == null ? null : t.hour * 60 + t.minute;
}

Widget dayToggles(int mask, ValueChanged<int> onChanged) => Wrap(children: [
      for (var d = 0; d < 7; d++) chip(weekDays[d], mask & (1 << d) != 0, () => onChanged(mask ^ (1 << d))),
    ]);

// ---------------------------------------------------------------------------------------
// The programme

/// The programme as the editor holds it, ready to send.
Map<String, dynamic> freshProgramme() => {
      'zone_name': DateTime.now().timeZoneName,
      'zone': zoneOffsets(),
      'mode': {'kind': 'shuffle'},
      'hours': [],
      'slots': [],
      'ident': null,
      'ident_every_min': 30,
    };

class ProgrammeEditor extends StatefulWidget {
  final String station;
  const ProgrammeEditor({super.key, required this.station});
  @override
  State<ProgrammeEditor> createState() => _ProgrammeEditorState();
}

class _ProgrammeEditorState extends State<ProgrammeEditor> {
  Map<String, dynamic> p = freshProgramme();
  List library = [];
  List hands = [];
  String? saved;
  bool loaded = false;

  @override
  void initState() {
    super.initState();
    Native.callAway({'cmd': 'programme', 'station': widget.station}).then((r) {
      if (!mounted || r is! Map) return;
      setState(() {
        library = (r['library'] as List?) ?? [];
        hands = (r['hands'] as List?) ?? [];
        if (r['programme'] is Map) p = Map<String, dynamic>.from(r['programme']);
        loaded = true;
      });
    });
  }

  Map get mode => p['mode'] as Map;
  List get hours => p['hours'] as List;
  List get slots => p['slots'] as List;

  Future<void> save() async {
    p['zone'] = zoneOffsets();
    p['zone_name'] = DateTime.now().timeZoneName;
    final r = await Native.callAway({'cmd': 'programme_set', 'station': widget.station, 'programme': p});
    setState(() => saved = r is Map && r['error'] != null ? r['error'] : 'Set. Every listener\'s clock follows it within a minute.');
  }

  @override
  Widget build(BuildContext context) {
    final kind = mode['kind'] ?? 'shuffle';
    final start = (mode['start_min'] as num?)?.toInt() ?? 18 * 60;
    return ListView(padding: const EdgeInsets.fromLTRB(24, 8, 24, 48), children: [
      Text('THE PROGRAMME', style: carved(20)),
      note('How your station fills the day, in your own time (${zoneLabel()}). It keeps doing this for as long as anyone carries it, whether or not you ever open the app again.'),
      if (!loaded) Padding(padding: const EdgeInsets.only(top: 16), child: note('Reading the log')),
      const SizedBox(height: 18),
      heading('HOW IT PLAYS'),
      Wrap(children: [
        for (final (k, label) in [('shuffle', 'Shuffled'), ('in_order', 'In order'), ('newest_first', 'Newest first'), ('daily', 'A daily loop'), ('weekly', 'A weekly loop')])
          chip(label, kind == k, () => setState(() => p['mode'] = {'kind': k, 'start_min': start, 'day': mode['day'] ?? 0})),
      ]),
      note(switch (kind) {
        'in_order' => 'Every recording in the order you added them, round and round, from the top each Monday.',
        'newest_first' => 'The newest recording first, then back through the rest. Good for a podcast.',
        'daily' => 'A day\'s tape: from the top at the same time every day. Shorter than a day, it goes round again.',
        'weekly' => 'A week\'s tape: from the top at the same time every week.',
        _ => 'Every recording once a round, in a fresh order, before any plays twice.',
      }),
      if (kind == 'daily' || kind == 'weekly')
        Row(children: [
          if (kind == 'weekly')
            DropdownButton<int>(
              value: ((mode['day'] as num?)?.toInt() ?? 0) % 7,
              dropdownColor: panel,
              items: [for (var d = 0; d < 7; d++) DropdownMenuItem(value: d, child: Text(weekDayNames[d], style: serif(15)))],
              onChanged: (d) => setState(() => mode['day'] = d),
            ),
          const SizedBox(width: 12),
          pyrateButton('FROM ${hhmm(start)}', () async {
            final m = await pickTime(context, start);
            if (m != null) setState(() => mode['start_min'] = m);
          }),
        ]),
      const SizedBox(height: 22),
      heading('ON THE AIR'),
      Wrap(children: [
        chip('Round the clock', hours.isEmpty, () => setState(() => p['hours'] = [])),
        chip('Set hours', hours.isNotEmpty, () => setState(() => p['hours'] = [{'days': 0x7f, 'start_min': 18 * 60, 'len_min': 360}])),
      ]),
      for (final (i, h) in hours.indexed) ...[
        dayToggles((h['days'] as num).toInt(), (v) => setState(() => h['days'] = v)),
        Row(children: [
          pyrateButton('FROM ${hhmm((h['start_min'] as num).toInt())}', () async {
            final m = await pickTime(context, (h['start_min'] as num).toInt());
            if (m != null) setState(() => h['start_min'] = m);
          }),
          const SizedBox(width: 10),
          Expanded(
            child: Slider(
              value: ((h['len_min'] as num).toInt() / 60).clamp(1, 23).toDouble(),
              min: 1, max: 23, divisions: 22, activeColor: bone, inactiveColor: boneGhost,
              label: lengthLabel((h['len_min'] as num).toInt()),
              onChanged: (v) => setState(() => h['len_min'] = (v.round()) * 60),
            ),
          ),
          if (hours.length > 1) IconButton(icon: const Icon(Icons.close, size: 16, color: boneGhost), onPressed: () => setState(() => hours.removeAt(i))),
        ]),
        note('${daysLabel((h['days'] as num).toInt())} from ${hhmm((h['start_min'] as num).toInt())} for ${lengthLabel((h['len_min'] as num).toInt())}, then quiet until the next.'),
      ],
      if (hours.isNotEmpty) TextButton(onPressed: () => setState(() => hours.add({'days': 0x60, 'start_min': 10 * 60, 'len_min': 240})), child: Text('ANOTHER SPELL', style: carved(9.5, color: boneFaint))),
      const SizedBox(height: 22),
      heading('REGULAR SHOWS'),
      note('A slot of the week that is always the same: a show, a hand\'s watch, or the hour you go live. Outside them, the library plays as above.'),
      for (final (i, sl) in slots.indexed)
        ListTile(
          contentPadding: EdgeInsets.zero,
          title: Row(children: [
            Flexible(child: Text(sl['title'] ?? '', style: serif(17))),
            if (sl['live'] == true) Padding(padding: const EdgeInsets.only(left: 8), child: Text('LIVE', style: carved(8.5, color: bloodBright, spacing: 0.3))),
          ]),
          subtitle: Text('${daysLabel((sl['span']['days'] as num).toInt())} · ${hhmm((sl['span']['start_min'] as num).toInt())} · ${lengthLabel((sl['span']['len_min'] as num).toInt())}', style: mono(10, color: boneFaint)),
          trailing: IconButton(icon: const Icon(Icons.close, size: 16, color: boneGhost), onPressed: () => setState(() => slots.removeAt(i))),
          onTap: () async {
            final r = await pyrateSheet<Map<String, dynamic>>(context, SlotEditor(slot: Map<String, dynamic>.from(sl), library: library, hands: hands));
            if (r != null) setState(() => slots[i] = r);
          },
        ),
      TextButton(
        onPressed: () async {
          final r = await pyrateSheet<Map<String, dynamic>>(context, SlotEditor(library: library, hands: hands));
          if (r != null) setState(() => slots.add(r));
        },
        child: Text('ADD A REGULAR SHOW', style: carved(10, color: bone)),
      ),
      const SizedBox(height: 22),
      heading('THE BELL'),
      note('A short recording, your ident, played on the bell.'),
      DropdownButton<String?>(
        isExpanded: true,
        value: library.any((t) => t['track'] == p['ident']) ? p['ident'] as String? : null,
        dropdownColor: panel,
        hint: Text('No ident', style: serif(15, color: boneFaint)),
        items: [
          DropdownMenuItem<String?>(value: null, child: Text('No ident', style: serif(15, color: boneFaint))),
          for (final t in library) DropdownMenuItem<String?>(value: t['track'], child: Text(t['title'] ?? '', overflow: TextOverflow.ellipsis, style: serif(15))),
        ],
        onChanged: (v) => setState(() => p['ident'] = v),
      ),
      if (p['ident'] != null)
        Wrap(children: [for (final m in [15, 30, 60]) chip('Every $m minutes', (p['ident_every_min'] as num?)?.toInt() == m, () => setState(() => p['ident_every_min'] = m))]),
      const SizedBox(height: 24),
      pyrateButton('SET THE PROGRAMME', loaded ? save : null),
      if (saved != null) Padding(padding: const EdgeInsets.only(top: 10), child: note(saved!)),
    ]);
  }
}

class SlotEditor extends StatefulWidget {
  final Map<String, dynamic>? slot;
  final List library;
  final List hands;
  const SlotEditor({super.key, this.slot, required this.library, required this.hands});
  @override
  State<SlotEditor> createState() => _SlotEditorState();
}

class _SlotEditorState extends State<SlotEditor> {
  late final title = TextEditingController(text: widget.slot?['title'] ?? '');
  late final about = TextEditingController(text: widget.slot?['about'] ?? '');
  late int days = (widget.slot?['span']?['days'] as num?)?.toInt() ?? 0x7f;
  late int start = (widget.slot?['span']?['start_min'] as num?)?.toInt() ?? 18 * 60;
  late int len = (widget.slot?['span']?['len_min'] as num?)?.toInt() ?? 60;
  late bool live = widget.slot?['live'] == true;
  late String fill = (widget.slot?['fill']?['kind'] ?? 'library').toString();
  late List<String> picked = ((widget.slot?['fill']?['tracks'] as List?) ?? []).cast<String>().toList();
  late bool shuffle = widget.slot?['fill']?['shuffle'] == true;
  late String? hand = widget.slot?['fill']?['key'] as String?;
  late int newest = (widget.slot?['fill']?['count'] as num?)?.toInt() ?? 3;

  @override
  Widget build(BuildContext context) => Padding(
        padding: EdgeInsets.fromLTRB(24, 22, 24, 24 + MediaQuery.of(context).viewInsets.bottom),
        child: ListView(shrinkWrap: true, children: [
          heading('A REGULAR SHOW'),
          TextField(controller: title, maxLength: 80, style: serif(18), decoration: pyrateField('What it is called')),
          TextField(controller: about, maxLength: 400, style: serif(15), decoration: pyrateField('A line about it')),
          dayToggles(days, (v) => setState(() => days = v)),
          Row(children: [
            pyrateButton('AT ${hhmm(start)}', () async {
              final m = await pickTime(context, start);
              if (m != null) setState(() => start = m);
            }),
          ]),
          const SizedBox(height: 8),
          Wrap(children: [for (final m in [15, 30, 60, 90, 120, 180, 240, 360]) chip(lengthLabel(m), len == m, () => setState(() => len = m))]),
          pyrateSwitch(live, (v) => setState(() => live = v), 'I mean to go live in it'),
          note(live ? 'Listeners see LIVE on the schedule and can ring a bell. If you are not on air, what follows plays instead.' : 'What plays in it:'),
          Wrap(children: [
            for (final (k, label) in [('library', 'The library, shuffled'), ('tracks', 'Chosen recordings'), ('newest', 'The newest'), if (widget.hands.isNotEmpty) ('hand', 'A hand\'s watch')])
              chip(label, fill == k, () => setState(() => fill = k)),
          ]),
          if (fill == 'tracks') ...[
            pyrateSwitch(shuffle, (v) => setState(() => shuffle = v), 'Shuffle them'),
            TrackPicker(library: widget.library, picked: picked, onChanged: (v) => setState(() => picked = v)),
          ],
          if (fill == 'newest') Wrap(children: [for (final n in [1, 3, 5, 10]) chip('The newest $n', newest == n, () => setState(() => newest = n))]),
          if (fill == 'hand')
            Wrap(children: [for (final h in widget.hands) chip((h['label'] ?? 'A hand').toString(), hand == h['key'], () => setState(() => hand = h['key']))]),
          if (fill == 'hand') note('The watch plays what that hand adds from their own phone.'),
          const SizedBox(height: 14),
          pyrateButton('KEEP IT', () {
            Navigator.pop(context, <String, dynamic>{
              'title': title.text.trim().isEmpty ? (live ? 'Live' : 'A show') : title.text.trim(),
              'about': about.text.trim(),
              'span': {'days': days == 0 ? 0x7f : days, 'start_min': start, 'len_min': len},
              'live': live,
              'fill': switch (fill) {
                'tracks' => {'kind': 'tracks', 'tracks': picked, 'shuffle': shuffle},
                'newest' => {'kind': 'newest', 'count': newest},
                'hand' when hand != null => {'kind': 'hand', 'key': hand},
                _ => {'kind': 'library'},
              },
            });
          }),
        ]),
      );
}

// ---------------------------------------------------------------------------------------
// The shapes a new station can start in

/// How a new station starts. Chosen when the colours go up; all of it can change later.
class StartShape {
  String kind = 'shuffle';
  int start = 18 * 60;
  int hours = 0; // 0: round the clock.
  int day = 0;
  String feed = '';
  int feedCount = 5;
  int liveAt = 18 * 60;
  int liveLen = 60;

  Map<String, dynamic> programme() {
    final p = freshProgramme();
    p['mode'] = switch (kind) {
      'daily' => {'kind': 'daily', 'start_min': start},
      'weekly' => {'kind': 'weekly', 'day': day, 'start_min': start},
      'in_order' => {'kind': 'in_order'},
      'feed' => {'kind': 'newest_first'},
      _ => {'kind': 'shuffle'},
    };
    if (hours > 0) p['hours'] = [{'days': 0x7f, 'start_min': start, 'len_min': hours * 60}];
    if (kind == 'live') {
      p['slots'] = [
        {'title': 'Live', 'about': '', 'span': {'days': 0x7f, 'start_min': liveAt, 'len_min': liveLen}, 'live': true, 'fill': {'kind': 'library'}},
      ];
    }
    return p;
  }

  /// Puts the shape on a station that has just raised its colours.
  Future<void> apply(String station) async {
    await Native.callAway({'cmd': 'programme_set', 'station': station, 'programme': programme()});
    if (kind == 'feed' && feed.trim().isNotEmpty) {
      await Feeds.add(station, feed.trim(), feedCount);
      unawaited(Feeds.refresh());
    }
  }
}

class StartShapePicker extends StatefulWidget {
  final StartShape shape;
  const StartShapePicker({super.key, required this.shape});
  @override
  State<StartShapePicker> createState() => _StartShapePickerState();
}

class _StartShapePickerState extends State<StartShapePicker> {
  final feed = TextEditingController();

  @override
  Widget build(BuildContext context) {
    final s = widget.shape;
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Text('WHAT IT PLAYS', style: carved(12, color: boneFaint, spacing: 0.3)),
      const SizedBox(height: 8),
      Wrap(children: [
        for (final (k, label) in [('shuffle', 'Round the clock'), ('in_order', 'In order'), ('daily', 'A daily loop'), ('weekly', 'A weekly loop'), ('feed', 'A podcast feed'), ('live', 'Live every day')])
          chip(label, s.kind == k, () => setState(() => s.kind = k)),
      ]),
      note(switch (s.kind) {
        'in_order' => 'Your recordings in the order you add them, round and round.',
        'daily' => 'A day\'s tape, from the top at the same time every day.',
        'weekly' => 'A week of programmes, from the top at the same time every week.',
        'feed' => 'Paste a podcast\'s feed. New episodes come aboard by themselves, newest first.',
        'live' => 'Your recordings fill the day, and the same hour every day is kept for you to go live.',
        _ => 'Everything you add, shuffled, all day and all night.',
      }),
      if (s.kind == 'daily' || s.kind == 'weekly') ...[
        if (s.kind == 'weekly')
          Wrap(children: [for (var d = 0; d < 7; d++) chip(weekDays[d], s.day == d, () => setState(() => s.day = d))]),
        Row(children: [
          pyrateButton('FROM ${hhmm(s.start)}', () async {
            final m = await pickTime(context, s.start);
            if (m != null) setState(() => s.start = m);
          }),
        ]),
        if (s.kind == 'daily') ...[
          const SizedBox(height: 8),
          Wrap(children: [for (final h in [0, 2, 4, 6, 8, 12]) chip(h == 0 ? 'All day' : '$h hours a day', s.hours == h, () => setState(() => s.hours = h))]),
        ],
      ],
      if (s.kind == 'feed') ...[
        TextField(controller: feed, onChanged: (v) => s.feed = v, style: mono(12, color: bone), decoration: pyrateField('https://…/feed.xml')),
        Wrap(children: [for (final n in [1, 5, 10, 25]) chip('Keep the newest $n', s.feedCount == n, () => setState(() => s.feedCount = n))]),
      ],
      if (s.kind == 'live') ...[
        Row(children: [
          pyrateButton('LIVE AT ${hhmm(s.liveAt)}', () async {
            final m = await pickTime(context, s.liveAt);
            if (m != null) setState(() => s.liveAt = m);
          }),
        ]),
        Wrap(children: [for (final m in [30, 60, 120]) chip(lengthLabel(m), s.liveLen == m, () => setState(() => s.liveLen = m))]),
      ],
    ]);
  }
}

// ---------------------------------------------------------------------------------------
// Podcast feeds: new episodes come aboard by themselves

class Feeds {
  static List load() {
    final v = pref('feeds');
    return v is List ? v : [];
  }

  static Future<void> add(String station, String url, int count) async {
    final all = load().where((f) => f['url'] != url).toList();
    all.add({'station': station, 'url': url, 'count': count, 'seen': []});
    pref('feeds', all);
  }

  static Future<void> remove(String url) async => pref('feeds', load().where((f) => f['url'] != url).toList());

  static bool _busy = false;

  /// Fetches every feed and brings aboard episodes not yet set, newest first, up to each
  /// feed's count. Runs while the app is open; a Lantern does the same with feed2folder.
  static Future<String> refresh() async {
    if (_busy) return 'Already reading the feeds';
    _busy = true;
    var added = 0;
    try {
      final all = load();
      for (final f in all) {
        final items = await _items(f['url'] as String);
        final seen = ((f['seen'] as List?) ?? []).cast<String>().toSet();
        final fresh = items.where((i) => !seen.contains(i.$1)).take((f['count'] as num?)?.toInt() ?? 5).toList();
        for (final (guid, title, url, show) in fresh.reversed) {
          final path = await _download(url);
          if (path == null) continue;
          final r = await Native.callAway({'cmd': 'add_track', 'station': f['station'], 'path': path, 'title': title, 'by': show, 'source': f['url'], 'talk': true});
          // One at a time: a phone setting five episodes at once would cook.
          final job = r is Map ? r['job'] : null;
          for (var i = 0; job != null && i < 900; i++) {
            await Future.delayed(const Duration(seconds: 2));
            final st = Native.call({'cmd': 'status'});
            final j = st is Map ? ((st['jobs'] as List?) ?? []).firstWhere((x) => x['id'] == job, orElse: () => null) : null;
            if (j == null || !(j['state'] as String).startsWith('Setting')) break;
          }
          unawaited(File(path).delete().then((_) {}, onError: (_) {}));
          seen.add(guid);
          added++;
          f['seen'] = seen.toList();
          pref('feeds', all);
        }
        f['seen'] = seen.toList();
      }
      pref('feeds', all);
    } catch (e) {
      return 'A feed could not be read: $e';
    } finally {
      _busy = false;
    }
    return added == 0 ? 'Nothing new in the feeds' : '$added new ${added == 1 ? 'episode' : 'episodes'} coming aboard';
  }

  /// (guid, title, enclosure url, show title), newest first, from an RSS feed.
  static Future<List<(String, String, String, String)>> _items(String url) async {
    final client = HttpClient()..connectionTimeout = const Duration(seconds: 20);
    try {
      final rq = await client.getUrl(Uri.parse(url));
      final rs = await rq.close();
      final body = await rs.transform(utf8.decoder).join();
      String unx(String s) => s.replaceAll(RegExp(r'<!\[CDATA\[|\]\]>'), '').replaceAll('&amp;', '&').replaceAll('&lt;', '<').replaceAll('&gt;', '>').replaceAll('&quot;', '"').replaceAll('&#39;', "'").trim();
      String tag(String s, String t) => unx(RegExp('<$t[^>]*>([\\s\\S]*?)</$t>').firstMatch(s)?.group(1) ?? '');
      final show = tag(body.split(RegExp('<item[ >]')).first, 'title');
      final out = <(String, String, String, String)>[];
      for (final m in RegExp(r'<item[ >]([\s\S]*?)</item>').allMatches(body)) {
        final it = m.group(1)!;
        final enc = RegExp('<enclosure[^>]*url="([^"]+)"').firstMatch(it)?.group(1);
        if (enc == null) continue;
        final guid = tag(it, 'guid');
        out.add((guid.isEmpty ? enc : guid, tag(it, 'title'), unx(enc), show));
      }
      return out;
    } finally {
      client.close();
    }
  }

  static Future<String?> _download(String url) async {
    final client = HttpClient()..connectionTimeout = const Duration(seconds: 30);
    try {
      var uri = Uri.parse(url);
      for (var hops = 0; hops < 6; hops++) {
        final rq = await client.getUrl(uri);
        rq.followRedirects = true;
        final rs = await rq.close();
        if (rs.statusCode >= 300 && rs.statusCode < 400 && rs.headers.value('location') != null) {
          uri = uri.resolve(rs.headers.value('location')!);
          continue;
        }
        if (rs.statusCode != 200) return null;
        final dir = await getTemporaryDirectory();
        final f = File('${dir.path}/feed-${DateTime.now().microsecondsSinceEpoch}.audio');
        await rs.pipe(f.openWrite());
        return f.path;
      }
      return null;
    } catch (_) {
      return null;
    } finally {
      client.close();
    }
  }
}

class FeedsPanel extends StatefulWidget {
  final String station;
  const FeedsPanel({super.key, required this.station});
  @override
  State<FeedsPanel> createState() => _FeedsPanelState();
}

class _FeedsPanelState extends State<FeedsPanel> {
  final url = TextEditingController();
  String? msg;

  @override
  Widget build(BuildContext context) {
    final mine = Feeds.load().where((f) => f['station'] == widget.station).toList();
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      heading('PODCAST FEEDS'),
      note('New episodes come aboard by themselves whenever this phone has the app open. A Lantern can do it round the clock.'),
      for (final f in mine)
        Row(children: [
          Expanded(child: Text(f['url'], maxLines: 1, overflow: TextOverflow.ellipsis, style: mono(11, color: boneFaint))),
          IconButton(icon: const Icon(Icons.close, size: 16, color: boneGhost), onPressed: () async {
            await Feeds.remove(f['url']);
            setState(() {});
          }),
        ]),
      TextField(controller: url, style: mono(12, color: bone), decoration: pyrateField('A podcast feed URL')),
      const SizedBox(height: 10),
      Wrap(spacing: 6, children: [
        pyrateButton('ADD THE FEED', () async {
          if (url.text.trim().isEmpty) return;
          await Feeds.add(widget.station, url.text.trim(), 5);
          url.clear();
          setState(() => msg = 'Reading it now');
          final r = await Feeds.refresh();
          if (mounted) setState(() => msg = r);
        }),
        if (mine.isNotEmpty)
          pyrateButton('CHECK NOW', () async {
            setState(() => msg = 'Reading the feeds');
            final r = await Feeds.refresh();
            if (mounted) setState(() => msg = r);
          }),
      ]),
      if (msg != null) Padding(padding: const EdgeInsets.only(top: 8), child: note(msg!)),
    ]);
  }
}

// ---------------------------------------------------------------------------------------
// A poster: the station's name, its place on the dial and a code to scan. Put it up around
// town, and whoever scans it is listening a tap later, in the browser or the app.

/// Where a station sits on every dial, from its key: 87.5 to 108.0.
double frequency(String id) => (875 + int.parse(id.substring(0, 6), radix: 16) % 206) / 10;

String stationLink(String id) => 'https://hostis.black/pyrateradio/listen/#$id';

class QrPainter extends CustomPainter {
  final int n;
  final String bits;
  final Color ink;
  QrPainter(this.n, this.bits, this.ink);

  @override
  void paint(Canvas canvas, Size size) {
    final cell = size.width / (n + 8);
    final p = Paint()..color = ink;
    for (var y = 0; y < n; y++) {
      for (var x = 0; x < n; x++) {
        if (bits[y * n + x] == '1') canvas.drawRect(Rect.fromLTWH((x + 4) * cell, (y + 4) * cell, cell + 0.4, cell + 0.4), p);
      }
    }
  }

  @override
  bool shouldRepaint(covariant QrPainter old) => old.bits != bits || old.ink != ink;
}

class PosterSheet extends StatefulWidget {
  final Map station;
  const PosterSheet({super.key, required this.station});
  @override
  State<PosterSheet> createState() => _PosterSheetState();
}

class _PosterSheetState extends State<PosterSheet> {
  final key = GlobalKey();
  bool paper = true;
  String? msg;

  Future<void> share() async {
    final ro = key.currentContext?.findRenderObject() as RenderRepaintBoundary?;
    if (ro == null) return;
    final img = await ro.toImage(pixelRatio: 5);
    final png = await img.toByteData(format: ui.ImageByteFormat.png);
    if (png == null) return;
    final dir = Directory('${(await getTemporaryDirectory()).path}/share');
    if (!dir.existsSync()) dir.createSync(recursive: true);
    final f = File('${dir.path}/${safeName(widget.station['name'] ?? 'Pyrate Radio')} poster.png');
    await f.writeAsBytes(png.buffer.asUint8List());
    await ship.invokeMethod('share_file', {'path': f.path, 'mime': 'image/png'});
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.station;
    final id = s['id'] as String;
    final q = Native.call({'cmd': 'qr', 'text': stationLink(id)});
    final ink = paper ? const Color(0xFF0A0A0A) : bone;
    final ground = paper ? const Color(0xFFFBF8F1) : black;
    final soft = paper ? const Color(0xFF55524B) : boneFaint;
    return Padding(
      padding: const EdgeInsets.fromLTRB(24, 20, 24, 24),
      child: SingleChildScrollView(
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          heading('A POSTER'),
          note('Print it and put it up. Whoever scans the code is listening a tap later: in the browser on any phone, or in the app if they have it. No account, no store.'),
          const SizedBox(height: 10),
          Wrap(children: [chip('For paper', paper, () => setState(() => paper = true)), chip('For screens', !paper, () => setState(() => paper = false))]),
          const SizedBox(height: 8),
          Center(
            child: RepaintBoundary(
              key: key,
              child: Container(
                width: 300,
                height: 424,
                color: ground,
                padding: const EdgeInsets.fromLTRB(22, 26, 22, 18),
                child: Column(children: [
                  Text('PYRATE RADIO', style: carved(13, color: ink, spacing: 0.4)),
                  const SizedBox(height: 14),
                  Text(s['name'] ?? '', textAlign: TextAlign.center, maxLines: 2, overflow: TextOverflow.ellipsis, style: serif(30, color: ink)),
                  const SizedBox(height: 4),
                  Text('${frequency(id).toStringAsFixed(1)} on the dial', style: mono(12, color: soft, weight: FontWeight.w700)),
                  if ((s['about'] ?? '').toString().isNotEmpty)
                    Padding(
                      padding: const EdgeInsets.only(top: 8),
                      child: Text(s['about'], textAlign: TextAlign.center, maxLines: 2, overflow: TextOverflow.ellipsis, style: serif(13, color: soft, italic: true)),
                    ),
                  const Spacer(),
                  if (q is Map)
                    SizedBox(
                      width: 196,
                      height: 196,
                      child: CustomPaint(painter: QrPainter((q['size'] as num).toInt(), q['bits'] as String, ink)),
                    ),
                  const Spacer(),
                  Text('Scan to listen. No app, no account.', style: serif(14, color: ink)),
                  const SizedBox(height: 4),
                  Text('hostis.black/pyrateradio', style: mono(10, color: soft)),
                ]),
              ),
            ),
          ),
          const SizedBox(height: 16),
          Row(children: [
            pyrateButton('SHARE THE POSTER', share),
            const SizedBox(width: 8),
            pyrateButton('THE LINK', () => ship.invokeMethod('share_text', {'text': 'Tune in to ${s['name']} on Pyrate Radio: ${stationLink(id)}'})),
          ]),
          if (msg != null) Padding(padding: const EdgeInsets.only(top: 8), child: note(msg!)),
        ]),
      ),
    );
  }
}

// ---------------------------------------------------------------------------------------
// Chatter: talk in the station's room, cheer what is on, ask for a recording. Lines pass
// between the people in the room and are kept by nobody.

class ChatterPanel extends StatefulWidget {
  final String station;
  final bool own;
  /// When the recording now playing ends: where a request the keeper takes goes on.
  final int? nextAt;
  const ChatterPanel({super.key, required this.station, required this.own, this.nextAt});
  @override
  State<ChatterPanel> createState() => _ChatterPanelState();
}

class _ChatterPanelState extends State<ChatterPanel> {
  Map? room;
  Timer? tick;
  bool joining = false;
  String? err;
  final say = TextEditingController();
  late final name = TextEditingController(text: (pref('chat_name') ?? '').toString());

  @override
  void initState() {
    super.initState();
    read();
    tick = Timer.periodic(const Duration(seconds: 2), (_) => read());
  }

  @override
  void dispose() {
    tick?.cancel();
    super.dispose();
  }

  Future<void> read() async {
    final r = await Native.callAway({'cmd': 'chat_read', 'station': widget.station});
    if (mounted && r is Map) setState(() => room = r);
  }

  Set<String> get mutes => ((pref('chat_mutes') as List?) ?? []).cast<String>().toSet();

  Future<void> send(String kind, {String text = '', String? track}) async {
    final r = await Native.callAway({'cmd': 'chat_send', 'station': widget.station, 'kind': kind, 'text': text, if (track != null) 'track': track, 'name': name.text.trim()});
    if (r is Map && r['error'] != null) setState(() => err = r['error']);
    read();
  }

  Future<void> ask() async {
    final lib = await Native.callAway({'cmd': 'library', 'station': widget.station});
    if (!mounted || lib is! List || lib.isEmpty) return;
    final note2 = TextEditingController();
    final track = await pyrateSheet<String>(
      context,
      Padding(
        padding: const EdgeInsets.fromLTRB(24, 20, 24, 24),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          heading('ASK FOR A RECORDING'),
          TextField(controller: note2, maxLength: 140, style: serif(15), decoration: pyrateField('A word with it, if you like')),
          Flexible(
            child: ListView(shrinkWrap: true, children: [
              for (final t in lib)
                ListTile(
                  contentPadding: EdgeInsets.zero,
                  title: Text(t['title'] ?? '', style: serif(16)),
                  subtitle: (t['by'] ?? '').toString().isEmpty ? null : Text(t['by'], style: carved(9, color: boneFaint)),
                  onTap: () => Navigator.pop(context, t['hash'] as String),
                ),
            ]),
          ),
        ]),
      ),
    );
    if (track != null) send('request', text: note2.text.trim(), track: track);
  }

  @override
  Widget build(BuildContext context) {
    final r = room;
    if (r == null || r['joined'] != true) {
      return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        note('Talk with whoever else is listening, cheer what is on, or ask the station for a recording. Lines are kept by nobody: they pass between the people in the room and are gone. Joining tells the others in the room that this phone is there, which listening alone never does.'),
        const SizedBox(height: 10),
        pyrateButton(joining ? 'JOINING' : 'JOIN THE CHATTER', joining
            ? null
            : () async {
                setState(() => joining = true);
                await Native.callAway({'cmd': 'chat_join', 'station': widget.station});
                if (mounted) setState(() => joining = false);
                read();
              }),
      ]);
    }
    final m = mutes;
    final lines = ((r['lines'] as List?) ?? []).where((l) => !m.contains(l['voice'])).toList();
    final shown = lines.length > 60 ? lines.sublist(lines.length - 60) : lines;
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      note('You speak here as ${name.text.trim().isEmpty ? r['voice'] : name.text.trim()}. ${(r['neighbours'] ?? 0) > 0 ? 'Linked to ${r['neighbours']} in the room.' : 'Looking for others in the room.'}'),
      const SizedBox(height: 8),
      if (shown.isEmpty) note('Nobody has said anything yet.'),
      for (final l in shown)
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 5),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Row(children: [
              GestureDetector(
                onLongPress: l['mine'] == true
                    ? null
                    : () async {
                        final ok = await showDialog<bool>(
                          context: context,
                          builder: (c) => AlertDialog(
                            backgroundColor: panel,
                            title: Text('Mute ${l['name'] == '' ? l['voice'] : l['name']}?', style: serif(20)),
                            content: Text('Only you stop hearing them, on this phone.', style: serif(14, color: boneFaint, italic: true)),
                            actions: [TextButton(onPressed: () => Navigator.pop(c, true), child: Text('MUTE', style: carved(11)))],
                          ),
                        );
                        if (ok == true) {
                          pref('chat_mutes', [...m, l['voice']]);
                          read();
                        }
                      },
                child: Text('${(l['name'] ?? '').toString().isEmpty ? l['voice'] : l['name']}${l['keeper'] == true ? ' (the station)' : ''}'.toUpperCase(),
                    style: carved(9, color: l['keeper'] == true ? bloodBright : (l['mine'] == true ? bone : boneFaint), spacing: 0.25)),
              ),
              const SizedBox(width: 8),
              Text(clockTime((l['at_ms'] as num).toInt()), style: mono(9, color: boneGhost)),
            ]),
            if (l['kind'] == 'cheer') Text('Cheers to what is on', style: serif(15, color: bone, italic: true)),
            if (l['kind'] == 'request')
              Row(children: [
                Expanded(child: Text('Asks for ${l['title'] ?? 'a recording'}${(l['text'] ?? '').toString().isNotEmpty ? ', "${l['text']}"' : ''}', style: serif(15, color: bone, italic: true))),
                if (widget.own && l['track'] != null)
                  TextButton(
                    onPressed: () async {
                      await Native.callAway({
                        'cmd': 'show_add', 'station': widget.station, 'title': 'By request', 'about': 'Asked for by ${(l['name'] ?? '').toString().isEmpty ? l['voice'] : l['name']}',
                        'at_ms': widget.nextAt ?? DateTime.now().millisecondsSinceEpoch + 60000, 'tracks': [l['track']], 'live_min': 0,
                      });
                      if (context.mounted) toast(context, 'It plays next.');
                    },
                    child: Text('PLAY IT NEXT', style: carved(9, color: bloodBright)),
                  ),
              ]),
            if (l['kind'] == 'text') Text(l['text'] ?? '', style: serif(15, color: boneDim)),
          ]),
        ),
      const SizedBox(height: 8),
      Row(children: [
        Expanded(child: TextField(controller: say, maxLength: 280, style: serif(16), decoration: pyrateField('Say something'), onSubmitted: (t) {
          if (t.trim().isEmpty) return;
          say.clear();
          send('text', text: t.trim());
        })),
        const SizedBox(width: 8),
        pyrateButton('SAY', () {
          final t = say.text.trim();
          if (t.isEmpty) return;
          say.clear();
          send('text', text: t);
        }),
      ]),
      Wrap(spacing: 6, runSpacing: 6, children: [
        pyrateButton('CHEERS', () => send('cheer')),
        pyrateButton('ASK FOR ONE', ask),
      ]),
      TextField(controller: name, maxLength: 32, style: serif(15), decoration: pyrateField('Your name in the room'), onChanged: (v) => pref('chat_name', v.trim())),
      if (err != null) note(err!),
    ]);
  }
}
