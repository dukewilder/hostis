// Pyrate Radio: a station's own page, its programme, its shows and notices, and the
// shapes a new station can start in. No rights reserved.

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:isolate';
import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart' show rootBundle;

import 'art.dart';
import 'main.dart';
import 'more.dart';
import 'native.dart';
import 'shell.dart';

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
  if (p == null) return 'Plays everything shuffled, day and night.';
  final zone = (p['zone_name'] ?? '').toString();
  final at = zone.isEmpty ? '' : ' $zone';
  final m = (p['mode'] as Map?) ?? {};
  final start = (m['start_min'] as num?)?.toInt() ?? 0;
  var s = switch (m['kind']) {
    'in_order' => 'Plays everything in order',
    'newest_first' => 'Plays its newest recordings first',
    'daily' => 'Starts from the top every day at ${hhmm(start)}$at',
    'weekly' => 'Starts from the top every ${weekDayNames[((m['day'] as num?)?.toInt() ?? 0) % 7]} at ${hhmm(start)}$at',
    _ => 'Plays everything shuffled',
  };
  final hours = (p['hours'] as List?) ?? [];
  if (hours.isEmpty) {
    s += ', day and night.';
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
      bells = bellIds(b);
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

  Widget section(String title) => Padding(padding: const EdgeInsets.only(top: 26, bottom: 8), child: Text(title, style: carved(11, color: boneFaint, spacing: 0.45)));

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
                Row(children: [Flexible(child: Text(s['name'] ?? '', style: pica(28, spacing: 0.03))), ...seals(s, size: 9)]),
                if ((s['port'] ?? '').toString().isNotEmpty) Text((s['port']).toString().toUpperCase(), style: carved(9, color: boneFaint, spacing: 0.25)),
                if (((s['kinds'] as List?) ?? []).isNotEmpty) Text(((s['kinds'] as List).join(' · ')).toUpperCase(), style: carved(9, color: boneGhost, spacing: 0.25)),
              ]),
            ),
          ]),
          if ((s['about'] ?? '').toString().isNotEmpty) Padding(padding: const EdgeInsets.only(top: 14), child: Text(s['about'], style: serif(16, color: boneDim, italic: true))),
          // The call sign is a name; the key is the station. Two stations can wear the same
          // call sign, never the same key.
          Padding(
            padding: const EdgeInsets.only(top: 10),
            child: SelectableText('KEY  ${RegExp(r'.{1,4}').allMatches(id).map((m) => m.group(0)).join(' ')}', style: mono(10, color: boneFaint)),
          ),
          const SizedBox(height: 14),
          Wrap(spacing: 4, runSpacing: 4, children: [
            pyrateButton('▶︎ LISTEN', () {
              widget.radio.tune(id);
              Navigator.pop(context);
            }),
            // A pocket station's whereabouts go only by invitation, never by a public link or poster.
            if (s['pocket'] != true) pyrateButton('PASS IT ON', () => shareStation(s)),
            if ((s['tip'] ?? '').toString().isNotEmpty) pyrateButton('TIP', () => tipStation(context, s['tip'], name: (s['name'] ?? 'the station').toString())),
            if (s['pocket'] != true) pyrateButton('A POSTER', () => pyrateSheet(context, PosterSheet(station: s))),
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
                await Navigator.push(context, MaterialPageRoute(builder: (_) => Scaffold(backgroundColor: black, appBar: AppBar(backgroundColor: black), body: ProgrammeEditor(station: id, name: (s['name'] ?? '').toString()))));
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
              if ((p['now']['slot'] ?? '').toString().isNotEmpty) Text((p['now']['slot']).toString().toUpperCase(), style: carved(10, color: boneDim, spacing: 0.3)),
              Text(p['now']['title'] ?? '', style: serif(18, color: boneDim, italic: true)),
              if ((p['now']['by'] ?? '').toString().isNotEmpty) Text(p['now']['by'], style: carved(10, color: boneFaint, spacing: 0.2)),
            ] else
              note(p['back_at'] != null
                  ? offAirLabel((p['back_at'] as num).toInt())
                  : ((p['tracks'] as num?) ?? 0) == 0
                      ? 'Nothing scheduled yet.'
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
              Padding(padding: const EdgeInsets.only(top: 8), child: note(own ? 'No shows yet. Add a regular show in the schedule, or plan a premiere.' : 'No shows on the bill this week.')),
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
                final id2 = bellId(id, start);
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
                        icon: Icon(ringing ? Icons.notifications_active : Icons.notifications_none, size: 18, color: ringing ? bone : boneGhost),
                        onPressed: () async {
                          if (ringing) {
                            await ship.invokeMethod('bell_cancel', {'id': id2, 'station': id, 'at': start});
                            if (mounted) setState(() => bells.remove(id2));
                          } else {
                            await ringAt(context, id: id2, at: start, title: (l['title'] ?? 'A show').toString(), station: id, name: (s['name'] ?? '').toString());
                            if (mounted) setState(() => bells.add(id2));
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
  void dispose() {
    text.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Padding(
        padding: EdgeInsets.fromLTRB(24, 22, 24, 24 + MediaQuery.of(context).viewInsets.bottom),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          heading('A NOTICE'),
          note('A word to everyone who opens your station\'s page. It travels in the log like everything else.'),
          TextField(enableIMEPersonalizedLearning: false, controller: text, maxLines: 4, maxLength: 600, style: serif(17), decoration: pyrateField('What you want to say')),
          Wrap(children: [
            for (final (d, label) in [(0, 'Until I take it down'), (1, 'A day'), (7, 'A week')]) chip(label, days == d, () => setState(() => days = d)),
          ]),
          const SizedBox(height: 12),
          pyrateButton('POST IT', () async {
            final until = days == 0 ? 0 : DateTime.now().millisecondsSinceEpoch + days * 86400000;
            final r = await Native.callAway({'cmd': 'notice_post', 'station': widget.station, 'text': text.text, 'until_ms': until});
            if (r is Map && r['error'] != null) {
              if (mounted) setState(() => err = r['error']);
            } else if (context.mounted) {
              Navigator.pop(context);
            }
          }),
          if (err != null) Padding(padding: const EdgeInsets.only(top: 8), child: Text(err!, style: serif(14, color: bone, italic: true))),
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
                  child: picked.contains(t['track']) ? Text('${picked.indexOf(t['track']) + 1}', style: mono(12, color: bone, weight: FontWeight.w700)) : const Text('·', style: TextStyle(color: boneGhost)),
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
  void dispose() {
    title.dispose();
    about.dispose();
    super.dispose();
  }

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
          note('A one-off airing at a set time. Every listener hears it at the same moment, and anyone who keeps the station can set a reminder.'),
          TextField(enableIMEPersonalizedLearning: false, controller: title, maxLength: 80, style: serif(18), decoration: pyrateField('What it is called')),
          TextField(enableIMEPersonalizedLearning: false, controller: about, maxLength: 400, style: serif(15), decoration: pyrateField('A line about it')),
          Row(children: [
            Text('${dayLabel(when.millisecondsSinceEpoch)} ${clockTime(when.millisecondsSinceEpoch)}', style: mono(14, color: bone)),
            const Spacer(),
            pyrateButton('WHEN', () async {
              final d = await showDatePicker(context: context, initialDate: when, firstDate: DateTime.now(), lastDate: DateTime.now().add(const Duration(days: 365)));
              if (d == null || !context.mounted) return;
              final t = await showTimePicker(context: context, initialTime: TimeOfDay.fromDateTime(when));
              if (t == null) return;
              if (mounted) setState(() => when = DateTime(d.year, d.month, d.day, t.hour, t.minute));
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
              if (mounted) setState(() => err = r['error']);
            } else if (context.mounted) {
              Navigator.pop(context);
            }
          }),
          if (err != null) Padding(padding: const EdgeInsets.only(top: 8), child: Text(err!, style: serif(14, color: bone, italic: true))),
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
  final String name;
  const ProgrammeEditor({super.key, required this.station, this.name = ''});
  @override
  State<ProgrammeEditor> createState() => _ProgrammeEditorState();
}

class _ProgrammeEditorState extends State<ProgrammeEditor> {
  Map<String, dynamic> p = freshProgramme();
  List library = [];
  List hands = [];
  String? saved;
  bool loaded = false;
  String? speaking;

  String? loadError;

  @override
  void initState() {
    super.initState();
    load();
  }

  /// Only a whole answer counts as read: a failed read leaves SET THE PROGRAMME shut, so
  /// a blank editor can never be saved over the programme already on the air.
  void load() {
    setState(() => loadError = null);
    Native.callAway({'cmd': 'programme', 'station': widget.station}).then((r) {
      if (!mounted) return;
      if (r is! Map || r['error'] != null || r['library'] is! List) {
        setState(() => loadError = (r is Map ? r['error'] : null)?.toString() ?? 'no answer');
        return;
      }
      setState(() {
        library = r['library'] as List;
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
    // The zone goes out only when the programme keeps clock times. A station that only
    // shuffles says nothing about where its keeper lives.
    final clock = hours.isNotEmpty || slots.isNotEmpty;
    p['zone'] = clock ? zoneOffsets() : [];
    p['zone_name'] = clock ? DateTime.now().timeZoneName : '';
    final r = await Native.callAway({'cmd': 'programme_set', 'station': widget.station, 'programme': p});
    if (!mounted) return;
    setState(() => saved = r is Map && r['error'] != null ? r['error'] : 'Set. Every listener\'s clock follows it within a minute.');
  }

  @override
  Widget build(BuildContext context) {
    final kind = mode['kind'] ?? 'shuffle';
    final start = (mode['start_min'] as num?)?.toInt() ?? 18 * 60;
    return ListView(padding: const EdgeInsets.fromLTRB(24, 8, 24, 48), children: [
      masthead('The Programme'),
      note('How your station fills the day, in your own time (${zoneLabel()}). Clock times carry that zone with them, where anyone can read it. It keeps doing this for as long as anyone carries it, whether or not you ever open the app again.'),
      if (!loaded && loadError == null) Padding(padding: const EdgeInsets.only(top: 16), child: note('Reading the log')),
      // The time-zone table a programme carries runs out some day; past it the local time
      // keeps its last offset. Setting the programme again writes a fresh forty years.
      if (loaded)
        Builder(builder: (_) {
          final z = p['zone'];
          final last = z is List && z.isNotEmpty && z.last is List ? ((z.last as List).first as num?)?.toInt() ?? 0 : 0;
          final soon = DateTime.now().add(const Duration(days: 730)).millisecondsSinceEpoch;
          if (last == 0 || last > soon) return const SizedBox();
          return Padding(
            padding: const EdgeInsets.only(top: 12),
            child: note('Your station knows your time zone\'s changes only until ${DateTime.fromMillisecondsSinceEpoch(last).year}. Save the schedule again before then to keep local times right.'),
          );
        }),
      if (loadError != null)
        Padding(
          padding: const EdgeInsets.only(top: 16),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            note('The schedule could not be loaded ($loadError). Try again in a moment.'),
            const SizedBox(height: 8),
            pyrateButton('TRY AGAIN', load),
          ]),
        ),
      const SizedBox(height: 18),
      heading('HOW IT PLAYS'),
      Wrap(children: [
        for (final (k, label) in [('shuffle', 'Shuffle'), ('in_order', 'In order'), ('newest_first', 'Newest first'), ('daily', 'Same time daily'), ('weekly', 'Same time weekly')])
          chip(label, kind == k, () => setState(() => p['mode'] = {'kind': k, 'start_min': start, 'day': mode['day'] ?? 0})),
      ]),
      note(switch (kind) {
        'in_order' => 'Every recording in the order you added them, over and over.',
        'newest_first' => 'The newest recording first, then the rest. Good for a podcast.',
        'daily' => 'Starts from the top at the same time every day, and repeats if it runs short.',
        'weekly' => 'Starts from the top at the same time every week.',
        _ => 'Everything in a fresh order. Nothing plays twice until everything has played once.',
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
            if (m != null && mounted) setState(() => mode['start_min'] = m);
          }),
        ]),
      const SizedBox(height: 22),
      heading('HOURS'),
      Wrap(children: [
        chip('All day', hours.isEmpty, () => setState(() => p['hours'] = [])),
        chip('Set hours', hours.isNotEmpty, () => setState(() => p['hours'] = [{'days': 0x7f, 'start_min': 18 * 60, 'len_min': 360}])),
      ]),
      for (final (i, h) in hours.indexed) ...[
        dayToggles((h['days'] as num).toInt(), (v) => setState(() => h['days'] = v)),
        Row(children: [
          pyrateButton('FROM ${hhmm((h['start_min'] as num).toInt())}', () async {
            final m = await pickTime(context, (h['start_min'] as num).toInt());
            if (m != null && mounted) setState(() => h['start_min'] = m);
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
      if (hours.isNotEmpty) TextButton(onPressed: () => setState(() => hours.add({'days': 0x60, 'start_min': 10 * 60, 'len_min': 240})), child: Text('ADD MORE HOURS', style: carved(9.5, color: boneFaint))),
      const SizedBox(height: 22),
      heading('REGULAR SHOWS'),
      note('A show at the same time each week, a co-host\'s slot, or the hour you go live. The rest of the time, everything plays as above.'),
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
            if (r != null && mounted) setState(() => slots[i] = r);
          },
        ),
      TextButton(
        onPressed: () async {
          final r = await pyrateSheet<Map<String, dynamic>>(context, SlotEditor(library: library, hands: hands));
          if (r != null && mounted) setState(() => slots.add(r));
        },
        child: Text('ADD A REGULAR SHOW', style: carved(10, color: bone)),
      ),
      const SizedBox(height: 22),
      heading('STATION NAME ON THE HOUR'),
      note('A short recording, such as your station\'s name, played on a regular beat.'),
      DropdownButton<String?>(
        isExpanded: true,
        value: library.any((t) => t['track'] == p['ident']) ? p['ident'] as String? : null,
        dropdownColor: panel,
        hint: Text('None', style: serif(15, color: boneFaint)),
        items: [
          DropdownMenuItem<String?>(value: null, child: Text('None', style: serif(15, color: boneFaint))),
          for (final t in library) DropdownMenuItem<String?>(value: t['track'], child: Text(t['title'] ?? '', overflow: TextOverflow.ellipsis, style: serif(15))),
        ],
        onChanged: (v) => setState(() => p['ident'] = v),
      ),
      if (p['ident'] != null)
        Wrap(children: [for (final m in [15, 30, 60]) chip('Every $m minutes', (p['ident_every_min'] as num?)?.toInt() == m, () => setState(() => p['ident_every_min'] = m))]),
      const SizedBox(height: 6),
      pyrateButton(speaking ?? 'MAKE ONE FROM TEXT', speaking != null ? null : () async {
        final text = await askText(context, 'YOUR STATION NAME', 'What the voice says', 'You\'re listening to ${widget.name.isEmpty ? 'Pyrate Radio' : widget.name}, ${frequency(widget.station).toStringAsFixed(1)} on the dial.');
        if (text == null || text.trim().isEmpty) return;
        if (mounted) setState(() => speaking = 'SPEAKING');
        final path = await speakToFile(text.trim());
        if (path == null) {
          if (mounted) setState(() => speaking = null);
          if (context.mounted) toast(context, 'This phone has no voice to speak with. Install a text-to-speech voice in the phone\'s settings.');
          return;
        }
        setState(() => speaking = 'ADDING IT');
        final title = 'Ident: ${text.trim().length > 40 ? '${text.trim().substring(0, 40)}…' : text.trim()}';
        final hash = await addAndWait(widget.station, path, title, 'spoken ident');
        final r = await Native.callAway({'cmd': 'programme', 'station': widget.station});
        if (!mounted) return;
        setState(() {
          speaking = null;
          if (r is Map) library = (r['library'] as List?) ?? library;
          if (hash != null) {
            p['ident'] = hash;
            p['ident_every_min'] = (p['ident_every_min'] as num?)?.toInt() ?? 30;
          }
        });
      }),
      note('The phone reads it aloud in its own voice. Save the schedule to keep it.'),
      const SizedBox(height: 24),
      pyrateButton('SAVE THE SCHEDULE', loaded ? save : null),
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
  void dispose() {
    title.dispose();
    about.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Padding(
        padding: EdgeInsets.fromLTRB(24, 22, 24, 24 + MediaQuery.of(context).viewInsets.bottom),
        child: ListView(shrinkWrap: true, children: [
          heading('A REGULAR SHOW'),
          TextField(enableIMEPersonalizedLearning: false, controller: title, maxLength: 80, style: serif(18), decoration: pyrateField('What it is called')),
          TextField(enableIMEPersonalizedLearning: false, controller: about, maxLength: 400, style: serif(15), decoration: pyrateField('A line about it')),
          dayToggles(days, (v) => setState(() => days = v)),
          Row(children: [
            pyrateButton('AT ${hhmm(start)}', () async {
              final m = await pickTime(context, start);
              if (m != null && mounted) setState(() => start = m);
            }),
          ]),
          const SizedBox(height: 8),
          Wrap(children: [for (final m in [15, 30, 60, 90, 120, 180, 240, 360]) chip(lengthLabel(m), len == m, () => setState(() => len = m))]),
          pyrateSwitch(live, (v) => setState(() => live = v), 'I will go live in this slot'),
          note(live ? 'Listeners see LIVE on the schedule and can set a reminder. If you are not live, your recordings play instead.' : 'What plays in it:'),
          Wrap(children: [
            for (final (k, label) in [('library', 'Everything, shuffled'), ('tracks', 'Chosen recordings'), ('newest', 'The newest'), if (widget.hands.isNotEmpty) ('hand', 'A co-host')])
              chip(label, fill == k, () => setState(() => fill = k)),
          ]),
          if (fill == 'tracks') ...[
            pyrateSwitch(shuffle, (v) => setState(() => shuffle = v), 'Shuffle them'),
            TrackPicker(library: widget.library, picked: picked, onChanged: (v) => setState(() => picked = v)),
          ],
          if (fill == 'newest') Wrap(children: [for (final n in [1, 3, 5, 10]) chip('The newest $n', newest == n, () => setState(() => newest = n))]),
          if (fill == 'hand')
            Wrap(children: [for (final h in widget.hands) chip((h['label'] ?? 'A co-host').toString(), hand == h['key'], () => setState(() => hand = h['key']))]),
          if (fill == 'hand') note('Plays what that co-host adds from their own phone.'),
          const SizedBox(height: 14),
          pyrateButton('DONE', () {
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
  void dispose() {
    feed.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.shape;
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Text('HOW IT PLAYS', style: carved(12, color: boneFaint, spacing: 0.3)),
      const SizedBox(height: 8),
      Wrap(children: [
        for (final (k, label) in [('shuffle', 'Shuffle'), ('in_order', 'In order'), ('daily', 'Same time daily'), ('weekly', 'Same time weekly'), ('feed', 'From a podcast'), ('live', 'With a live show')])
          chip(label, s.kind == k, () => setState(() => s.kind = k)),
      ]),
      note(switch (s.kind) {
        'in_order' => 'Your recordings in the order you add them, over and over.',
        'daily' => 'Your recordings start from the top at the same time every day.',
        'weekly' => 'Your recordings start from the top at the same time every week.',
        'feed' => 'Paste the feed link of your podcast, or one you have permission to play. New episodes are added by themselves. The podcast\'s host sees this phone\'s address, as with any podcast app.',
        'live' => 'Your recordings play all day, and the same hour each day is kept for you to go live.',
        _ => 'Everything you add, shuffled, day and night.',
      }),
      if (s.kind == 'daily' || s.kind == 'weekly') ...[
        if (s.kind == 'weekly')
          Wrap(children: [for (var d = 0; d < 7; d++) chip(weekDays[d], s.day == d, () => setState(() => s.day = d))]),
        Row(children: [
          pyrateButton('FROM ${hhmm(s.start)}', () async {
            final m = await pickTime(context, s.start);
            if (m != null && mounted) setState(() => s.start = m);
          }),
        ]),
        if (s.kind == 'daily') ...[
          const SizedBox(height: 8),
          Wrap(children: [for (final h in [0, 2, 4, 6, 8, 12]) chip(h == 0 ? 'All day' : '$h hours a day', s.hours == h, () => setState(() => s.hours = h))]),
        ],
      ],
      if (s.kind == 'feed') ...[
        TextField(enableIMEPersonalizedLearning: false, controller: feed, onChanged: (v) => s.feed = v, style: mono(12, color: bone), decoration: pyrateField('https://…/feed.xml')),
        Wrap(children: [for (final n in [1, 5, 10, 25]) chip('Keep the newest $n', s.feedCount == n, () => setState(() => s.feedCount = n))]),
      ],
      if (s.kind == 'live') ...[
        Row(children: [
          pyrateButton('LIVE AT ${hhmm(s.liveAt)}', () async {
            final m = await pickTime(context, s.liveAt);
            if (m != null && mounted) setState(() => s.liveAt = m);
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

  /// Writes one feed's memory back without touching the others, so a feed added or
  /// removed while the feeds are being read stays added or removed.
  static void _save(String url, Set<String> seen) {
    final all = load();
    for (final f in all) {
      if (f['url'] == url) f['seen'] = seen.toList();
    }
    pref('feeds', all);
  }

  static bool _busy = false;

  /// Fetches every feed and brings aboard episodes not yet set, up to each feed's count.
  /// A feed's first reading takes its newest few and passes over the back catalogue for
  /// good; after that only episodes newer than the last one taken come aboard. The phone
  /// reads the feeds every few hours on Wi-Fi while the app is open; a Lantern does the
  /// same with feed2folder.
  static Future<String> refresh() async {
    if (_busy) return 'Already reading the feeds';
    _busy = true;
    var added = 0;
    var failed = 0;
    try {
      for (final f in load()) {
        final url = f['url'] as String;
        final items = await _items(url);
        if (items == null) {
          failed++;
          continue;
        }
        final count = (f['count'] as num?)?.toInt() ?? 5;
        final seen = ((f['seen'] as List?) ?? []).cast<String>().toSet()..retainAll(items.map((i) => i.$1));
        final newest = items.indexWhere((i) => seen.contains(i.$1));
        final unseen = items.sublist(0, newest < 0 ? items.length : newest);
        for (final i in unseen.skip(count)) {
          seen.add(i.$1);
        }
        _save(url, seen);
        for (final (guid, title, enc, show) in unseen.take(count).toList().reversed) {
          if (!load().any((x) => x['url'] == url)) break;
          final path = await fetchOut(enc, maxBytes: 1 << 30);
          if (path == null) continue;
          final r = await Native.callAway({'cmd': 'add_track', 'station': f['station'], 'path': path, 'title': title, 'by': show, 'source': url, 'talk': true});
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
          _save(url, seen);
        }
      }
    } finally {
      _busy = false;
    }
    final got = added == 0 ? 'Nothing new in the feeds' : '$added new ${added == 1 ? 'episode' : 'episodes'} on the way';
    return failed == 0 ? got : '$got. ${failed == 1 ? 'One feed' : '$failed feeds'} could not be read.';
  }

  /// (guid, title, enclosure url, show title) in the feed's own order, newest first in any
  /// ordinary feed. Null when the feed cannot be read.
  static Future<List<(String, String, String, String)>?> _items(String url) async {
    final path = await fetchOut(url, maxBytes: 20000000);
    if (path == null) return null;
    final String body;
    try {
      body = utf8.decode(await File(path).readAsBytes(), allowMalformed: true);
    } finally {
      unawaited(File(path).delete().then((_) {}, onError: (_) {}));
    }
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
  void dispose() {
    url.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final mine = Feeds.load().where((f) => f['station'] == widget.station).toList();
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      heading('PODCAST FEEDS'),
      note('New episodes are added by themselves every few hours on Wi-Fi while the app is open. The podcast\'s host sees this phone\'s address, as with any podcast app.'),
      for (final f in mine)
        Row(children: [
          Expanded(child: Text(f['url'], maxLines: 1, overflow: TextOverflow.ellipsis, style: mono(11, color: boneFaint))),
          IconButton(icon: const Icon(Icons.close, size: 16, color: boneGhost), onPressed: () async {
            await Feeds.remove(f['url']);
            if (mounted) setState(() {});
          }),
        ]),
      TextField(enableIMEPersonalizedLearning: false, controller: url, style: mono(12, color: bone), decoration: pyrateField('A podcast feed URL')),
      const SizedBox(height: 10),
      Wrap(spacing: 6, children: [
        pyrateButton('ADD THE FEED', () async {
          if (url.text.trim().isEmpty) return;
          await Feeds.add(widget.station, url.text.trim(), 5);
          url.clear();
          if (mounted) setState(() => msg = 'Reading it now');
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

/// HOSTIS's own stations are passed on as hostis.black links; every other as an open player's.
String stationLink(String id) => isHarbour(id) || openPlayers.isEmpty ? 'https://hostis.black/pyrateradio/listen/#$id' : '${openPlayers.first}#$id';

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
    final dir = Directory('${(await scratchDir()).path}/share');
    if (!dir.existsSync()) dir.createSync(recursive: true);
    final f = File('${dir.path}/${safeName(widget.station['name'] ?? 'Pyrate Radio')} poster.png');
    await f.writeAsBytes(png.buffer.asUint8List());
    await ship.invokeMethod('share_file', {'path': f.path, 'mime': 'image/png'});
  }

  /// The whole station as one file, for an SD card, Bluetooth or the share sheet: it plays on
  /// any phone that opens it, with no network at all.
  Future<void> shareFile() async {
    setState(() => msg = 'Packing the station: its log, its sound and its art.');
    final dir = Directory('${(await scratchDir()).path}/share');
    if (!dir.existsSync()) dir.createSync(recursive: true);
    final path = '${dir.path}/${safeName(widget.station['name'] ?? 'Pyrate Radio')}.pyrate';
    final r = await Native.callAway({'cmd': 'pack_export', 'station': widget.station['id'], 'path': path});
    if (!mounted) return;
    if (r is! Map || r['error'] != null) {
      setState(() => msg = 'Not packed: ${r is Map ? r['error'] : 'try again'}');
      return;
    }
    setState(() => msg = (r['missing'] as num? ?? 0) > 0 ? '${r['missing']} pieces could not be found yet, so the file plays with gaps. The rest is in it.' : null);
    await ship.invokeMethod('share_file', {'path': path, 'mime': 'application/octet-stream'});
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
          note('Print it and put it up. Whoever scans the code is listening a tap later, in the browser on any phone or in the app if they have it.'),
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
                  Text(s['name'] ?? '', textAlign: TextAlign.center, maxLines: 2, overflow: TextOverflow.ellipsis, style: pica(30, color: ink, spacing: 0.03)),
                  const SizedBox(height: 4),
                  Text('${frequency(id).toStringAsFixed(1)} on the dial  ${(s['call_sign'] ?? '').toString()}', style: mono(12, color: soft, weight: FontWeight.w700)),
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
                  Text('Scan to listen in any browser.', style: serif(14, color: ink)),
                  const SizedBox(height: 4),
                  Text(isHarbour(id) ? 'hostis.black/pyrateradio, or search the call sign in any Pyrate Radio' : 'Or search the call sign in any Pyrate Radio', textAlign: TextAlign.center, style: mono(8.5, color: soft)),
                ]),
              ),
            ),
          ),
          const SizedBox(height: 16),
          Row(children: [
            pyrateButton('SHARE THE POSTER', share),
            const SizedBox(width: 8),
            pyrateButton('THE LINK', () => ship.invokeMethod('share_text', {'text': 'Tune in to ${s['name']} on Pyrate Radio: ${stationLink(id)}'})),
            const SizedBox(width: 8),
            pyrateButton('AS A FILE', shareFile),
          ]),
          if (msg != null) Padding(padding: const EdgeInsets.only(top: 8), child: note(msg!)),
        ]),
      ),
    );
  }
}

// ---------------------------------------------------------------------------------------
// Chatter: talk in the station's room, cheer what is on, ask for a recording. Lines pass
// between the people in the room; the radio writes none of them to disk.

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
  Timer? hailStop;
  bool hailing = false;

  /// A hail: up to twenty seconds of the listener's voice, sealed so only the keeper hears it.
  Future<void> hail() async {
    if (!hailing) {
      if (!await askMic(context)) return;
      final p = await ship.invokeMethod<String>('record_start', {'bitrate': 24000}).catchError((_) => null);
      if (p == null) {
        if (mounted) setState(() => err = 'The microphone would not start. Another app may have it.');
        return;
      }
      setState(() => hailing = true);
      hailStop = Timer(const Duration(seconds: 20), hail);
      return;
    }
    hailStop?.cancel();
    setState(() => hailing = false);
    final r = await ship.invokeMethod<Map>('record_stop').catchError((_) => null);
    final path = r?['path'] as String?;
    if (path == null) return;
    await send('hail', track: null, path: path);
    unawaited(File(path).delete().then((_) {}, onError: (_) {}));
    if (mounted && err == null) toast(context, 'Sent. Only the station can read it.');
  }
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
    hailStop?.cancel();
    if (hailing) ship.invokeMethod<Map>('record_stop').then((r) => dropCopy(r?['path'] as String?)).catchError((_) {});
    say.dispose();
    name.dispose();
    super.dispose();
  }

  Future<void> read() async {
    final r = await Native.callAway({'cmd': 'chat_read', 'station': widget.station});
    if (mounted && r is Map) setState(() => room = r);
  }

  Set<String> get mutes => ((pref('chat_mutes') as List?) ?? []).cast<String>().toSet();

  /// The voices muted on this phone, each to be heard again with a tap.
  Future<void> unmute() async {
    await pyrateSheet(
      context,
      StatefulBuilder(builder: (c, set) {
        final m = mutes.toList();
        return Padding(
          padding: const EdgeInsets.fromLTRB(24, 20, 24, 24),
          child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
            heading('MUTED VOICES'),
            if (m.isEmpty) note('Nobody is muted.'),
            for (final v in m)
              ListTile(
                contentPadding: EdgeInsets.zero,
                title: Text(v.toUpperCase(), style: carved(10, color: boneFaint, spacing: 0.25)),
                trailing: TextButton(
                  onPressed: () {
                    pref('chat_mutes', m.where((x) => x != v).toList());
                    set(() {});
                  },
                  child: Text('HEAR AGAIN', style: carved(9, color: bone)),
                ),
              ),
          ]),
        );
      }),
    );
    read();
  }

  Future<void> send(String kind, {String text = '', String? track, String? path}) async {
    final r = await Native.callAway({'cmd': 'chat_send', 'station': widget.station, 'kind': kind, 'text': text, if (track != null) 'track': track, if (path != null) 'path': path, 'mime': 'audio/mp4', 'name': name.text.trim()});
    if (r is Map && r['error'] != null && mounted) setState(() => err = r['error']);
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
          TextField(enableIMEPersonalizedLearning: false, controller: note2, maxLength: 140, style: serif(15), decoration: pyrateField('A word with it, if you like')),
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
        note('Chat with whoever else is listening, cheer what is on, or ask for a song. Nothing is saved, though anyone in the chat can copy what they read. People in the chat can tell this phone is there, and see its address unless the radio goes through Tor. Just listening shows nobody anything.'),
        const SizedBox(height: 10),
        pyrateButton(joining ? 'JOINING' : 'JOIN THE CHAT', joining
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
                    style: carved(9, color: l['keeper'] == true ? bone : (l['mine'] == true ? boneDim : boneFaint), spacing: 0.25)),
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
                    child: Text('PLAY IT NEXT', style: carved(9, color: bone)),
                  ),
              ]),
            if (l['kind'] == 'text') Text(l['text'] ?? '', style: serif(15, color: boneDim)),
            if (l['kind'] == 'hail')
              Row(children: [
                Expanded(child: Text('A private message for you', style: serif(15, color: bone, italic: true))),
                TextButton(
                  onPressed: () async {
                    final r = await Native.callAway({'cmd': 'chat_hail_open', 'station': widget.station, 'id': l['id']});
                    if (r is Map && r['path'] != null) {
                      await ship.invokeMethod('play_file', {'path': r['path']});
                    } else if (context.mounted) {
                      toast(context, (r is Map ? r['error'] : null) ?? 'That message would not open.');
                    }
                  },
                  child: Text('PLAY IT', style: carved(9, color: bone)),
                ),
              ]),
          ]),
        ),
      const SizedBox(height: 8),
      Row(children: [
        Expanded(child: TextField(enableIMEPersonalizedLearning: false, controller: say, maxLength: 280, style: serif(16), decoration: pyrateField('Say something'), onSubmitted: (t) {
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
        if (!widget.own) pyrateButton(hailing ? 'SEND' : 'MESSAGE THE STATION', hail, color: hailing ? bloodBright : bone),
      ]),
      TextField(enableIMEPersonalizedLearning: false, controller: name, maxLength: 32, style: serif(15), decoration: pyrateField('Your name in the chat'), onChanged: (v) => pref('chat_name', v.trim())),
      if (err != null) note(err!),
      Wrap(spacing: 6, runSpacing: 6, children: [
        pyrateButton('LEAVE THE CHAT', () async {
          await Native.callAway({'cmd': 'chat_leave', 'station': widget.station});
          read();
        }, color: boneFaint),
        if (m.isNotEmpty) pyrateButton('MUTED (${m.length})', () => unmute(), color: boneFaint),
      ]),
    ]);
  }
}

// ---------------------------------------------------------------------------------------
// The chart of the seas: every station that names a home port, where the port is. The
// world and its places are Natural Earth's, in the public domain, and ride in the app.

class Chart {
  final List<List<Offset>> land;
  final List places;
  Chart(this.land, this.places);
  static Chart? _c;
  static Future<Chart> load() async {
    if (_c != null) return _c!;
    final j = jsonDecode(await rootBundle.loadString('assets/chart.json')) as Map;
    final land = <List<Offset>>[];
    for (final ring in (j['land'] as String).split('M').where((r) => r.isNotEmpty)) {
      final n = ring.replaceAll('Z', '').trim().split(' ').map(double.parse).toList();
      land.add([for (var i = 0; i + 1 < n.length; i += 2) Offset(n[i], n[i + 1])]);
    }
    return _c = Chart(land, j['places'] as List);
  }

  final _hits = <String, Offset?>{};
  static String norm(String s) => s.toLowerCase().trim();

  /// "Austin" or "Austin, Texas": the most populous place that fits, in chart units.
  Offset? place(String port) => _hits.putIfAbsent(port, () {
        final parts = port.split(',').map(norm).toList();
        final n = parts.first, q = parts.skip(1).join(' ').trim();
        if (n.isEmpty) return null;
        for (final p in places) {
          if ((norm(p[0]) == n || norm(p[1]) == n) && (q.isEmpty || norm(p[2]).startsWith(q) || norm(p[3]).startsWith(q) || norm(p[4]) == q)) {
            return Offset(((p[6] as num) + 180) * 2, (90 - (p[5] as num)) * 2);
          }
        }
        return null;
      });
}

class ChartPainter extends CustomPainter {
  final Chart chart;
  final List<(Offset, Map)> dots;
  final String? tuned;
  ChartPainter(this.chart, this.dots, this.tuned);

  @override
  void paint(Canvas canvas, Size size) {
    final k = size.width / 720;
    canvas.translate(0, -20 * k);
    canvas.scale(k);
    final fill = Paint()..color = const Color(0x12EDE4CE);
    final edge = Paint()
      ..color = boneGhost
      ..style = PaintingStyle.stroke
      ..strokeWidth = 0.5;
    for (final ring in chart.land) {
      final path = Path()..addPolygon(ring, true);
      canvas.drawPath(path, fill);
      canvas.drawPath(path, edge);
    }
    for (final (o, s) in dots) {
      final on = s['id'] == tuned;
      canvas.drawCircle(o, on ? 5 : 3.4, Paint()..color = black);
      canvas.drawCircle(o, on ? 4 : 2.6, Paint()..color = s['live'] == true ? bloodBright : bone);
      if (on) {
        canvas.drawCircle(o, 6.5, Paint()
          ..color = bloodBright
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.2);
      }
    }
  }

  @override
  bool shouldRepaint(covariant ChartPainter old) => true;
}

class ChartPage extends StatefulWidget {
  final Tuner radio;
  final VoidCallback onTune;
  const ChartPage({super.key, required this.radio, required this.onTune});
  @override
  State<ChartPage> createState() => _ChartPageState();
}

class _ChartPageState extends State<ChartPage> {
  Chart? chart;

  @override
  void initState() {
    super.initState();
    Chart.load().then((c) => mounted ? setState(() => chart = c) : null);
  }

  @override
  Widget build(BuildContext context) {
    final c = chart;
    final hideMature = pref('hide_mature') == true, showExplicit = pref('show_explicit') == true;
    final dots = <(Offset, Map)>[];
    if (c != null) {
      for (final s in widget.radio.stations) {
        if ((s['explicit'] == true && !showExplicit) || (hideMature && (s['mature'] == true || s['explicit'] == true))) continue;
        final port = (s['port'] ?? '').toString();
        final o = port.isEmpty ? null : c.place(port);
        if (o == null) continue;
        final j = int.parse((s['id'] as String).substring(0, 4), radix: 16);
        dots.add((o + Offset((j % 7 - 3) * .6, ((j >> 3) % 7 - 3) * .6), s));
      }
    }
    return Scaffold(
      backgroundColor: black,
      appBar: AppBar(backgroundColor: black, title: Text('THE CHART OF THE SEAS', style: carved(13))),
      body: c == null
          ? Center(child: note('Unrolling the chart'))
          : ListView(padding: const EdgeInsets.fromLTRB(16, 8, 16, 32), children: [
              note(dots.isEmpty ? 'No station on the air has named a home port yet.' : '${dots.length} ${dots.length == 1 ? 'station names' : 'stations name'} a home port. Tap one to tune; pinch to look closer.'),
              const SizedBox(height: 12),
              LayoutBuilder(builder: (context, box) {
                final w = box.maxWidth, k = w / 720;
                return Container(
                  decoration: BoxDecoration(border: Border.all(color: boneGhost, width: 0.5)),
                  child: InteractiveViewer(
                    maxScale: 8,
                    child: GestureDetector(
                      onTapUp: (d) {
                        final at = Offset(d.localPosition.dx / k, d.localPosition.dy / k + 20);
                        (Offset, Map)? best;
                        for (final dot in dots) {
                          if (best == null || (dot.$1 - at).distance < (best.$1 - at).distance) best = dot;
                        }
                        if (best != null && (best.$1 - at).distance < 14) {
                          widget.radio.tune(best.$2['id'] as String);
                          widget.onTune();
                          Navigator.pop(context);
                        }
                      },
                      child: CustomPaint(size: Size(w, 290 * k), painter: ChartPainter(c, dots, widget.radio.tuned)),
                    ),
                  ),
                );
              }),
              const SizedBox(height: 16),
              for (final (_, s) in dots)
                ListTile(
                  contentPadding: EdgeInsets.zero,
                  title: Text(s['name'] ?? '', style: serif(17)),
                  subtitle: Text((s['port'] ?? '').toString().toUpperCase(), style: carved(9, color: boneFaint, spacing: 0.25)),
                  trailing: Text('${s['aboard'] ?? 0}', style: mono(13, color: boneDim, weight: FontWeight.w700)),
                  onTap: () {
                    widget.radio.tune(s['id'] as String);
                    widget.onTune();
                    Navigator.pop(context);
                  },
                ),
            ]),
    );
  }
}

// ---------------------------------------------------------------------------------------
// The phone's voice: idents, announcements and numbers, spoken here and set like any
// recording.

Future<String?> askText(BuildContext context, String title, String label, String initial) {
  final c = TextEditingController(text: initial);
  return showDialog<String>(
    context: context,
    builder: (d) => AlertDialog(
      backgroundColor: panel,
      shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
      title: Text(title, style: carved(13)),
      content: TextField(enableIMEPersonalizedLearning: false, controller: c, maxLines: 4, maxLength: 400, autofocus: true, style: serif(16), decoration: pyrateField(label)),
      actions: [TextButton(onPressed: () => Navigator.pop(d, c.text), child: Text('SPEAK IT', style: carved(11)))],
    ),
  );
}

/// Speaks text into a WAV file in the phone's temporary folder. Null if the phone has no voice.
Future<String?> speakToFile(String text, {double rate = 0.95, double pitch = 1.0}) async {
  final dir = await scratchDir();
  final path = '${dir.path}/voice-${DateTime.now().microsecondsSinceEpoch}.wav';
  final ok = await ship.invokeMethod<bool>('speak', {'text': text, 'path': path, 'rate': rate, 'pitch': pitch}).catchError((_) => false) ?? false;
  return ok && File(path).existsSync() && File(path).lengthSync() > 1000 ? path : null;
}

/// Sets a recording on a station and waits until it is on the clock. Its entry hash, or null.
Future<String?> addAndWait(String station, String path, String title, String source) async {
  final r = await Native.callAway({'cmd': 'add_track', 'station': station, 'path': path, 'title': title, 'by': '', 'source': source, 'talk': true});
  final job = r is Map ? r['job'] : null;
  for (var i = 0; job != null && i < 120; i++) {
    await Future.delayed(const Duration(milliseconds: 700));
    final st = Native.call({'cmd': 'status'});
    final j = st is Map ? ((st['jobs'] as List?) ?? []).firstWhere((x) => x['id'] == job, orElse: () => null) : null;
    if (j == null || !(j['state'] as String).startsWith('Setting')) break;
  }
  unawaited(File(path).delete().then((_) {}, onError: (_) {}));
  final lib = await Native.callAway({'cmd': 'library', 'station': station});
  if (lib is! List) return null;
  final hit = lib.lastWhere((t) => t['title'] == title, orElse: () => null);
  return hit?['hash'] as String?;
}

/// Writes 16-bit mono PCM as a WAV file, all at once from typed memory.
void writeWav(String path, Int16List pcm, int rate) {
  final h = ByteData(44);
  void tag(int o, String t) {
    for (var i = 0; i < 4; i++) {
      h.setUint8(o + i, t.codeUnitAt(i));
    }
  }
  tag(0, 'RIFF');
  h.setUint32(4, 36 + pcm.length * 2, Endian.little);
  tag(8, 'WAVE');
  tag(12, 'fmt ');
  h.setUint32(16, 16, Endian.little);
  h.setUint16(20, 1, Endian.little);
  h.setUint16(22, 1, Endian.little);
  h.setUint32(24, rate, Endian.little);
  h.setUint32(28, rate * 2, Endian.little);
  h.setUint16(32, 2, Endian.little);
  h.setUint16(34, 16, Endian.little);
  tag(36, 'data');
  h.setUint32(40, pcm.length * 2, Endian.little);
  final f = File(path).openSync(mode: FileMode.write);
  try {
    f.writeFromSync(h.buffer.asUint8List());
    final body = ByteData(pcm.length * 2);
    for (var i = 0; i < pcm.length; i++) {
      body.setInt16(i * 2, pcm[i], Endian.little);
    }
    f.writeFromSync(body.buffer.asUint8List());
  } finally {
    f.closeSync();
  }
}

/// The 16-bit mono samples and rate of a plain WAV file, as the phone's voice writes them
/// (the first channel, if it has more).
(Int16List, int)? readWav(String path) {
  final d = File(path).readAsBytesSync();
  if (d.length < 44 || String.fromCharCodes(d.sublist(0, 4)) != 'RIFF') return null;
  final v = ByteData.sublistView(d);
  var o = 12, rate = 0, ch = 1, bits = 16;
  while (o + 8 <= d.length) {
    final id = String.fromCharCodes(d.sublist(o, o + 4)), n = v.getUint32(o + 4, Endian.little);
    if (id == 'fmt ' && o + 24 <= d.length) {
      ch = math.max(1, v.getUint16(o + 10, Endian.little));
      rate = v.getUint32(o + 12, Endian.little);
      bits = v.getUint16(o + 22, Endian.little);
    } else if (id == 'data' && bits == 16) {
      final end = math.min(o + 8 + n, d.length);
      final count = (end - (o + 8)) ~/ (2 * ch);
      final out = Int16List(count);
      for (var k = 0; k < count; k++) {
        out[k] = v.getInt16(o + 8 + k * 2 * ch, Endian.little);
      }
      return (out, rate);
    }
    o += 8 + n + (n & 1);
  }
  return null;
}

/// The spoken message with the interval signal before it, made off the screen's thread.
/// The new file's path, or null if the voice's file could not be read.
Future<String?> withSignal(String spoken) => Isolate.run(() {
      final w = readWav(spoken);
      if (w == null || w.$2 <= 0) return null;
      final sig = intervalSignal(w.$2);
      final all = Int16List(sig.length + w.$1.length)
        ..setAll(0, sig)
        ..setAll(sig.length, w.$1);
      final path = spoken.replaceAll('.wav', '-signal.wav');
      writeWav(path, all, w.$2);
      return path;
    });

/// A numbers station's interval signal: three soft notes, twice, then quiet.
List<int> intervalSignal(int rate) {
  final out = <int>[];
  for (var round = 0; round < 2; round++) {
    for (final f in [660.0, 880.0, 660.0]) {
      final n = (rate * 0.32).round();
      for (var i = 0; i < n; i++) {
        final env = (i < 400 ? i / 400 : 1.0) * (i > n - 1200 ? (n - i) / 1200 : 1.0);
        out.add((math.sin(2 * math.pi * f * i / rate) * 9000 * env).round());
      }
    }
    out.addAll(List.filled((rate * 0.6).round(), 0));
  }
  return out;
}

const _digits = ['zero', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine'];

/// The numbers station kit: messages of five-figure groups in the phone's voice, each
/// after the interval signal. They go on the station's clock in order.
class NumbersKit extends StatefulWidget {
  final String station;
  const NumbersKit({super.key, required this.station});
  @override
  State<NumbersKit> createState() => _NumbersKitState();
}

class _NumbersKitState extends State<NumbersKit> {
  int messages = 3, groups = 8;
  String? busy;

  Future<void> make() async {
    final rnd = math.Random.secure();
    for (var m = 1; m <= messages; m++) {
      setState(() => busy = 'Reading message $m of $messages');
      final gs = [for (var g = 0; g < groups; g++) [for (var k = 0; k < 5; k++) rnd.nextInt(10)]];
      final text = 'Ready. Ready. Message ${_digits[m % 10]}. '
          '${gs.map((g) => g.map((d) => _digits[d]).join(', ')).join('. ')}. '
          'Again. ${gs.map((g) => g.map((d) => _digits[d]).join(', ')).join('. ')}. End of message.';
      final spoken = await speakToFile(text, rate: 0.8, pitch: 0.9);
      if (spoken == null) {
        if (mounted) setState(() => busy = null);
        if (mounted) toast(context, 'This phone has no voice to speak with. Install a text-to-speech voice in the phone\'s settings.');
        return;
      }
      var path = spoken;
      final signed = await withSignal(spoken);
      if (signed != null) {
        path = signed;
        unawaited(File(spoken).delete().then((_) {}, onError: (_) {}));
      }
      if (!mounted) return;
      setState(() => busy = 'Adding message $m');
      await addAndWait(widget.station, path, 'Message ${m.toString().padLeft(2, '0')}', 'numbers kit');
    }
    if (!mounted) return;
    setState(() => busy = null);
    toast(context, '$messages messages added to your station.');
    Navigator.pop(context);
  }

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.fromLTRB(24, 20, 24, 24),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          heading('A NUMBERS STATION'),
          note('Messages of five-figure groups, read twice in the phone\'s own voice after an interval signal, the way the shortwave stations did. The figures are drawn at random on this phone and mean whatever you say they mean.'),
          const SizedBox(height: 10),
          Text('MESSAGES', style: carved(10, color: boneFaint)),
          Wrap(children: [for (final n in [1, 3, 6, 12]) chip('$n', messages == n, () => setState(() => messages = n))]),
          Text('GROUPS IN EACH', style: carved(10, color: boneFaint)),
          Wrap(children: [for (final n in [5, 8, 12, 20]) chip('$n', groups == n, () => setState(() => groups = n))]),
          const SizedBox(height: 10),
          pyrateButton(busy ?? 'MAKE THEM', busy == null ? make : null),
        ]),
      );
}

// ---------------------------------------------------------------------------------------
// Flock: phones in one room playing as one. One hosts and shows a code; the others scan it
// with their cameras and fall into step with the host, on the host's station.

class FlockSheet extends StatefulWidget {
  const FlockSheet({super.key});
  @override
  State<FlockSheet> createState() => _FlockSheetState();
}

class _FlockSheetState extends State<FlockSheet> {
  Map st = {};
  String? link;
  final paste = TextEditingController();
  Timer? tick;

  @override
  void initState() {
    super.initState();
    read();
    tick = Timer.periodic(const Duration(seconds: 2), (_) => read());
  }

  @override
  void dispose() {
    tick?.cancel();
    paste.dispose();
    super.dispose();
  }

  void read() {
    final r = Native.call({'cmd': 'flock_status'});
    if (mounted && r is Map) {
      setState(() {
        st = r;
        link = r['link'] as String? ?? (r['hosting'] == true ? link : null);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final trim = (st['trim_ms'] as num?)?.toDouble() ?? 0;
    final q = link == null ? null : Native.call({'cmd': 'qr', 'text': link});
    return Padding(
      padding: EdgeInsets.fromLTRB(24, 20, 24, 24 + MediaQuery.of(context).viewInsets.bottom),
      child: SingleChildScrollView(
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          heading('PLAY ON SEVERAL PHONES'),
          note('Several phones in one room playing as one speaker. Host here, and the others scan the code with their cameras to play in step.'),
          const SizedBox(height: 12),
          if (st['hosting'] == true) ...[
            if (q is Map)
              Center(
                child: Container(
                  color: const Color(0xFFFBF8F1),
                  width: 220,
                  height: 220,
                  child: CustomPaint(painter: QrPainter((q['size'] as num).toInt(), q['bits'] as String, const Color(0xFF0A0A0A))),
                ),
              ),
            const SizedBox(height: 10),
            Wrap(spacing: 6, runSpacing: 6, children: [
              if (link != null) pyrateButton('SHARE THE LINK', () => ship.invokeMethod('share_text', {'text': 'Play along with me on Pyrate Radio: $link'})),
              pyrateButton('STOP HOSTING', () {
                Native.call({'cmd': 'flock_leave'});
                setState(() => link = null);
              }),
            ]),
          ] else if (st['joined'] == true) ...[
            note('Playing in step with the host.'),
            const SizedBox(height: 8),
            pyrateButton('LEAVE', () => Native.call({'cmd': 'flock_leave'})),
          ] else ...[
            pyrateButton('HOST HERE', () {
              final r = Native.call({'cmd': 'flock_host'});
              if (r is Map) setState(() => link = r['link'] as String?);
            }),
            const SizedBox(height: 12),
            TextField(enableIMEPersonalizedLearning: false, controller: paste, style: mono(11, color: bone), decoration: pyrateField('Or paste a link from the host')),
            const SizedBox(height: 8),
            pyrateButton('JOIN', () async {
              final r = await Native.callAway({'cmd': 'flock_join', 'link': paste.text.trim()});
              if (r is Map && r['error'] != null && context.mounted) toast(context, r['error']);
            }),
          ],
          const SizedBox(height: 18),
          Text('THIS PHONE\'S SPEAKER', style: carved(10, color: boneFaint)),
          note('If this phone sounds ahead of the others or behind them, as a Bluetooth speaker does, move it until they match.'),
          Slider(
            value: trim.clamp(-500, 500),
            min: -500, max: 500, divisions: 100, activeColor: bone, inactiveColor: boneGhost,
            label: '${trim.round()} ms',
            onChanged: (v) {
              Native.call({'cmd': 'flock_trim', 'ms': v.round()});
              read();
            },
          ),
          Text(trim == 0 ? 'As it is' : trim > 0 ? '${trim.round()} ms earlier' : '${(-trim).round()} ms later', style: mono(10, color: boneFaint)),
        ]),
      ),
    );
  }
}

// ---------------------------------------------------------------------------------------
// A pocket station's invitation: the link and its code carry the station's secret key.
// Whoever holds it can hear the station and carry it; hand it only to those you mean.

class InviteSheet extends StatelessWidget {
  final String station;
  const InviteSheet({super.key, required this.station});

  @override
  Widget build(BuildContext context) {
    final r = Native.call({'cmd': 'pocket_invite', 'station': station});
    final link = r is Map ? r['link'] as String? : null;
    final q = link == null ? null : Native.call({'cmd': 'qr', 'text': link});
    return Padding(
      padding: const EdgeInsets.fromLTRB(24, 20, 24, 24),
      child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
        heading('AN INVITATION'),
        note('Anyone with this can listen and pass it on, so send it only to people you trust. It works in the app and in any browser.'),
        const SizedBox(height: 12),
        if (q is Map)
          Center(
            child: Container(
              color: const Color(0xFFFBF8F1),
              width: 220,
              height: 220,
              child: CustomPaint(painter: QrPainter((q['size'] as num).toInt(), q['bits'] as String, const Color(0xFF0A0A0A))),
            ),
          ),
        const SizedBox(height: 12),
        if (link != null) pyrateButton('SEND THE INVITATION', () => ship.invokeMethod('share_text', {'text': link})),
        if (link == null) note('This station is on the public Log, so it has no invitation.'),
      ]),
    );
  }
}
