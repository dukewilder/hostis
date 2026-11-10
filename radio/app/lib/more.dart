// Pyrate Radio: the Log's filters, the watch bill, the tape, the hands and the ship's papers.
// No rights reserved.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';

import 'art.dart';
import 'main.dart';
import 'native.dart';
import 'station.dart';

const ship = MethodChannel('pyrate/service');

/// The kinds a station may call itself, for the Log's filters.
const stationKinds = ['talk', 'music', 'readings', 'news', 'sound'];

Widget pyrateButton(String label, VoidCallback? onTap, {Color color = bone}) => OutlinedButton(
      onPressed: onTap,
      style: OutlinedButton.styleFrom(
        side: BorderSide(color: onTap == null ? boneGhost : color.withValues(alpha: 0.7), width: 0.8),
        shape: const RoundedRectangleBorder(),
        padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 12),
      ),
      child: Text(label, style: carved(10.5, color: onTap == null ? boneGhost : color, spacing: 0.22)),
    );

InputDecoration pyrateField(String label) => InputDecoration(
      labelText: label,
      labelStyle: serif(15, color: boneFaint, italic: true),
      enabledBorder: const UnderlineInputBorder(borderSide: BorderSide(color: boneGhost)),
      focusedBorder: const UnderlineInputBorder(borderSide: BorderSide(color: bone)),
    );

Widget note(String text) => Text(text, style: serif(14, color: boneFaint, italic: true));

Widget heading(String text, {Color color = bone}) => Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Text(text, style: carved(14, color: color)),
    );

Widget pyrateSwitch(bool value, ValueChanged<bool> onChanged, String label) => Row(children: [
      Switch(value: value, onChanged: onChanged, activeThumbColor: bone, activeTrackColor: boneGhost, inactiveThumbColor: boneFaint, inactiveTrackColor: panel),
      Flexible(child: Text(label, style: mono(11))),
    ]);

Future<T?> pyrateSheet<T>(BuildContext context, Widget child) => showModalBottomSheet<T>(
      context: context,
      isScrollControlled: true,
      backgroundColor: panel,
      shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
      builder: (c) => SafeArea(child: ConstrainedBox(constraints: BoxConstraints(maxHeight: MediaQuery.of(c).size.height * 0.85), child: child)),
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
  final name = (s['name'] ?? '').toString().trim().toLowerCase();
  return name.isNotEmpty && harbourNames.contains(name);
}

final Set<String> harbourNames = {};

/// The seal on a station's name: HOSTIS's own, or a warning.
List<Widget> seals(Map s, {double size = 9}) => [
      if (isHarbour(s['id'] as String)) ...[
        const SizedBox(width: 8),
        Image.asset('assets/brand/mark.png', width: size * 1.7, height: size * 1.7, color: bone, colorBlendMode: BlendMode.modulate),
      ],
      if (falseColours(s)) ...[
        const SizedBox(width: 8),
        Text('FALSE COLOURS?', style: carved(size, color: bloodBright, spacing: 0.25)),
      ],
      if (s['explicit'] == true) ...[
        const SizedBox(width: 8),
        Text('EXPLICIT', style: carved(size, color: bloodBright, spacing: 0.25)),
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
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
            decoration: BoxDecoration(border: Border.all(color: on ? bone : boneGhost, width: on ? 0.9 : 0.5)),
            child: Text(label.toUpperCase(), style: carved(9.5, color: on ? bone : boneFaint, spacing: 0.22)),
          ),
        ),
      );

  void longPress(BuildContext context, Map<String, dynamic> s) {
    final id = s['id'] as String;
    pyrateSheet(
      context,
      SingleChildScrollView(
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          Padding(padding: const EdgeInsets.fromLTRB(24, 20, 24, 4), child: Row(children: [Flexible(child: Text(s['name'] ?? '', style: serif(22))), ...seals(s)])),
          Padding(padding: const EdgeInsets.fromLTRB(24, 0, 24, 8), child: Text('${s['call_sign']}  ·  Your chart is yours alone. Nothing here leaves this phone.', style: serif(13, color: boneFaint, italic: true))),
          ListTile(
            title: Text(s['kept'] == true ? 'STOP KEEPING' : 'KEEP', style: carved(12)),
            subtitle: Text('Kept stations sit at the top of the Log and the dial, and come aboard ahead of time on Wi-Fi.', style: serif(13, color: boneFaint, italic: true)),
            onTap: () {
              Native.call({'cmd': 'keep', 'station': id, 'value': s['kept'] != true});
              Navigator.pop(context);
            },
          ),
          ListTile(
            title: Text('THE PAGE', style: carved(12)),
            subtitle: Text('Notices, what is on now and next, and the week\'s shows.', style: serif(13, color: boneFaint, italic: true)),
            onTap: () {
              Navigator.pop(context);
              Navigator.push(context, MaterialPageRoute(builder: (_) => StationPage(station: s, radio: widget.radio)));
            },
          ),
          ListTile(
            title: Text('THE WATCH BILL', style: carved(12)),
            subtitle: Text('What plays today and this week, and a bell for any of it.', style: serif(13, color: boneFaint, italic: true)),
            onTap: () {
              Navigator.pop(context);
              pyrateSheet(context, WatchBill(station: s));
            },
          ),
          ListTile(
            title: Text('PASS THE STATION ON', style: carved(12)),
            subtitle: Text('A link that opens it in Pyrate Radio.', style: serif(13, color: boneFaint, italic: true)),
            onTap: () {
              Navigator.pop(context);
              shareStation(s);
            },
          ),
          ListTile(
            title: Text('MAROON', style: carved(12, color: bloodBright)),
            subtitle: Text('Gone from your Log and your dial, and dropped from your hold. Undo it under Colours.', style: serif(13, color: boneFaint, italic: true)),
            onTap: () {
              Native.call({'cmd': 'maroon', 'station': id, 'value': true});
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
            Text('THE LOG', style: carved(22, spacing: 0.3)),
            const SizedBox(height: 8),
            note('Every station on the air, your kept stations first. Hold a station to keep it, see its watch bill, pass it on or maroon it.'),
            if (radio.carrying > 0)
              Padding(
                padding: const EdgeInsets.only(top: 6),
                child: Text('Your ballast keeps ${radio.carrying} ${radio.carrying == 1 ? 'station' : 'stations'} afloat for the fleet.', style: mono(11, color: boneFaint)),
              ),
            const SizedBox(height: 10),
            Row(children: [
              Expanded(child: TextField(controller: search, style: serif(17), decoration: pyrateField('Search the Log'))),
              const SizedBox(width: 12),
              IconButton(
                tooltip: 'The chart of the seas',
                icon: const Icon(Icons.public, size: 20, color: boneFaint),
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
            Wrap(children: [
              for (final (k, label) in [('aboard', 'Most aboard'), ('longest', 'Longest on air'), ('newest', 'Newest'), ('carried', 'Most carried')])
                chip(label, sort == k, () => setState(() {
                      sort = k;
                      pref('sort', k);
                    })),
            ]),
            Wrap(children: [
              chip('on now', onNow, () => setState(() {
                    onNow = !onNow;
                    _nowTick = 0;
                  })),
              for (final k in stationKinds) chip(k, kind == k, () => setState(() => kind = kind == k ? null : k)),
              chip('hide mature', hideMature, () => setState(() {
                    hideMature = !hideMature;
                    pref('hide_mature', hideMature);
                  })),
            ]),
            if (ports.isNotEmpty)
              Wrap(children: [
                for (final p in ports) chip(p, port == p, () => setState(() => port = port == p ? null : p)),
              ]),
            const SizedBox(height: 6),
            const Divider(color: boneGhost, height: 1, thickness: 0.5),
          ]),
        ),
      ),
      if (st.isEmpty)
        SliverToBoxAdapter(
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Text(radio.stations.isEmpty ? 'Searching the Log' : onNow ? 'No shows and no one live at this minute. Spin the dial.' : 'No station answers to that.', style: serif(16, color: boneFaint, italic: true)),
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
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 14),
              child: Row(children: [
                StationArt(station: id, hash: s['cover'], width: 52, height: 52, fallback: Flag(id: id, width: 48, live: live)),
                const SizedBox(width: 16),
                Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Row(children: [
                      Flexible(child: Text(s['name'] ?? '', maxLines: 1, overflow: TextOverflow.ellipsis, style: serif(19, color: tuned ? bone : boneDim))),
                      if (live) ...[const SizedBox(width: 8), Text('ON AIR', style: carved(9, color: bloodBright, spacing: 0.3))],
                      if (s['kept'] == true) ...[const SizedBox(width: 8), Text('KEPT', style: carved(9, color: brass, spacing: 0.3))],
                      if (s['pocket'] == true) ...[const SizedBox(width: 8), Text('PRIVATE', style: carved(9, color: boneFaint, spacing: 0.3))],
                      ...seals(s),
                    ]),
                    const SizedBox(height: 2),
                    Text(s['call_sign'] ?? '', style: mono(10, color: boneGhost)),
                    if (on != null && (on['slot'] ?? '').toString().isNotEmpty)
                      Text((on['slot'] as String).toUpperCase(), maxLines: 1, overflow: TextOverflow.ellipsis, style: carved(9, color: bloodBright, spacing: 0.25)),
                    if (on != null && (on['title'] ?? '').toString().isNotEmpty)
                      Text('Now: ${on['title']}${(on['by'] ?? '').toString().isNotEmpty ? ' · ${on['by']}' : ''}',
                          maxLines: 1, overflow: TextOverflow.ellipsis, style: serif(13, color: boneDim, italic: true))
                    else if ((s['about'] ?? '').toString().isNotEmpty)
                      Text(s['about'], maxLines: 1, overflow: TextOverflow.ellipsis, style: serif(13, color: boneFaint, italic: true)),
                  ]),
                ),
                const SizedBox(width: 12),
                Column(crossAxisAlignment: CrossAxisAlignment.end, children: [
                  Text('${s['aboard'] ?? 0}', style: mono(16, color: tuned ? bone : boneDim, weight: FontWeight.w700)),
                  Text('ABOARD', style: carved(8, color: boneGhost, spacing: 0.2)),
                ]),
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

int bellId(String track, int start) => (track.hashCode ^ (start ~/ 1000)) & 0x7fffffff;

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
    setState(() {
      loading = false;
      if (r is List) {
        bill = r;
      } else {
        error = (r is Map ? r['error'] : null) ?? 'The watch bill has not arrived yet.';
      }
      bells = (jsonDecode(b) as List).map((x) => (x['id'] as num).toInt()).toSet();
    });
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.station;
    String? lastDay;
    return Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(24, 22, 24, 4),
        child: Text('THE WATCH BILL', style: carved(16)),
      ),
      Padding(
        padding: const EdgeInsets.fromLTRB(24, 0, 24, 10),
        child: note('${s['name']}, worked out on this phone from the station\'s own clock. Ring a bell and the phone tells you when it starts.'),
      ),
      if (loading) Padding(padding: const EdgeInsets.all(24), child: note('Reading the clock')),
      if (error != null) Padding(padding: const EdgeInsets.all(24), child: note(error!)),
      Flexible(
        child: ListView(shrinkWrap: true, padding: const EdgeInsets.fromLTRB(24, 0, 12, 24), children: [
          for (final item in bill) ...[
            if (dayLabel((item['start_ms'] as num).toInt()) != lastDay)
              Builder(builder: (_) {
                lastDay = dayLabel((item['start_ms'] as num).toInt());
                return Padding(padding: const EdgeInsets.only(top: 14, bottom: 4), child: Text(lastDay!, style: carved(10, color: brass, spacing: 0.3)));
              }),
            Builder(builder: (_) {
              final start = (item['start_ms'] as num).toInt();
              final id = bellId(item['track'] as String, start);
              final ringing = bells.contains(id);
              final past = start < DateTime.now().millisecondsSinceEpoch;
              return Row(children: [
                SizedBox(width: 54, child: Text(clockTime(start), style: mono(12, color: past ? bone : boneFaint, weight: past ? FontWeight.w700 : FontWeight.w400))),
                Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text(item['title'] ?? '', maxLines: 1, overflow: TextOverflow.ellipsis, style: serif(15, color: past ? bone : boneDim)),
                    if ((item['by'] ?? '').toString().isNotEmpty) Text(item['by'], style: carved(8.5, color: boneFaint, spacing: 0.2)),
                  ]),
                ),
                if (!past)
                  IconButton(
                    tooltip: ringing ? 'Silence the bell' : 'Ring a bell when it starts',
                    icon: Icon(ringing ? Icons.notifications_active : Icons.notifications_none, size: 18, color: ringing ? brass : boneGhost),
                    onPressed: () async {
                      if (ringing) {
                        await ship.invokeMethod('bell_cancel', {'id': id});
                        setState(() => bells.remove(id));
                      } else {
                        await ship.invokeMethod('bell_set', {'id': id, 'at': start, 'title': item['title'] ?? 'A show', 'station': s['id'], 'name': s['name']});
                        setState(() => bells.add(id));
                      }
                    },
                  )
                else
                  const SizedBox(width: 48, child: Center(child: Text('NOW', style: TextStyle(fontFamily: 'Cinzel', fontSize: 9, color: bloodBright, letterSpacing: 2)))),
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
  final dir = await getTemporaryDirectory();
  final share = Directory('${dir.path}/share');
  if (!share.existsSync()) share.createSync(recursive: true);
  final tmp = '${share.path}/tape-${DateTime.now().millisecondsSinceEpoch}.ogg';
  final r = Native.call({'cmd': 'tape', 'seconds': 120, 'path': tmp});
  if (r is! Map || r['error'] != null) {
    if (context.mounted) toast(context, 'Nothing to tape yet. Let it play a moment.');
    return;
  }
  final name = '${share.path}/${safeName('${r['station']} - ${r['title']}').isEmpty ? 'Pyrate Radio' : safeName('${r['station']} - ${r['title']}')}.ogg';
  File(tmp).renameSync(name);
  await ship.invokeMethod('share_file', {'path': name, 'mime': 'audio/ogg'});
}

void shareStation(Map s) {
  ship.invokeMethod('share_text', {
    'text': 'Tune in to ${s['name']} on Pyrate Radio: https://hostis.black/pyrateradio/listen/#${s['id']}',
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
  Widget build(BuildContext context) => Padding(
        padding: EdgeInsets.fromLTRB(24, 20, 24, 24 + MediaQuery.of(context).viewInsets.bottom),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          heading('A TIP FOR ${widget.name.toUpperCase()}'),
          note('It goes from your own wallet straight to the station. Pyrate Radio moves no money and sees none of it.'),
          const SizedBox(height: 10),
          SelectableText(widget.addr, style: mono(11, color: boneDim)),
          TextField(controller: memo, maxLength: 140, style: serif(16), decoration: pyrateField(lightning ? 'A note for the keeper (paste it in your wallet\'s comment)' : 'A note for the keeper')),
          Wrap(spacing: 6, runSpacing: 6, children: [
            pyrateButton('OPEN MY WALLET', () async {
              if (lightning && memo.text.trim().isNotEmpty) await Clipboard.setData(ClipboardData(text: memo.text.trim()));
              final ok = await ship.invokeMethod<bool>('open_uri', {'uri': tipUri(widget.addr, memo.text)}).catchError((_) => false) ?? false;
              if (!context.mounted) return;
              if (!ok) {
                await Clipboard.setData(ClipboardData(text: widget.addr));
                if (context.mounted) toast(context, 'No wallet on this phone answered. The address is copied.');
              } else if (lightning && memo.text.trim().isNotEmpty) {
                toast(context, 'Your note is copied: paste it in the wallet\'s comment.');
              }
            }),
            pyrateButton('COPY THE ADDRESS', () async {
              await Clipboard.setData(ClipboardData(text: widget.addr));
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
        note('Hand the radio itself to someone beside you. No store, no account, no internet needed: point their camera at the code while you are on the same Wi-Fi or hotspot, or send it by Quick Share.'),
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
          Text('LIGHTS OUT', style: carved(16)),
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
              }, color: bloodBright),
          ]),
        ]),
      ),
    );

Future<void> historySheet(BuildContext context, Tuner radio) => pyrateSheet(
      context,
      Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
        Padding(padding: const EdgeInsets.fromLTRB(24, 22, 24, 4), child: Text('HEARD TONIGHT', style: carved(16))),
        Padding(padding: const EdgeInsets.fromLTRB(24, 0, 24, 10), child: note('Kept in memory only, and gone when the app closes. Tape what you want to keep.')),
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
                      Text('${h['by'] ?? ''}${(h['by'] ?? '').toString().isNotEmpty ? ' · ' : ''}${h['station'] ?? ''}', style: carved(8.5, color: boneFaint, spacing: 0.2)),
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
    super.dispose();
  }

  Future<void> toggle() async {
    if (!recording) {
      final p = await ship.invokeMethod<String>('record_start').catchError((_) => null);
      if (p == null) {
        if (mounted) toast(context, 'Allow the microphone, then press again.');
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
      heading('RECORD A SEGMENT'),
      note('Talk into the phone and put it on the clock: a show, a reading, a word between records. No files needed.'),
      const SizedBox(height: 12),
      Row(children: [
        pyrateButton(recording ? '■  STOP  ${fmtTime(running)}' : '●  RECORD', toggle, color: recording ? bloodBright : bone),
      ]),
      if (path != null) ...[
        const SizedBox(height: 10),
        Text('${fmtTime(ms)} recorded', style: mono(11)),
        TextField(controller: title, style: serif(17), decoration: pyrateField('Title')),
        const SizedBox(height: 10),
        Row(children: [
          pyrateButton('PUT IT ON THE CLOCK', () {
            Native.call({
              'cmd': 'add_track',
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
          pyrateButton('DISCARD', () => setState(() => path = null), color: boneFaint),
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
      heading('THE STATION\'S PAPERS'),
      note('What your station is, in your own words. The Log\'s filters read them; nobody else can set them.'),
      const SizedBox(height: 10),
      Wrap(spacing: 8, runSpacing: 8, children: [
        for (final k in stationKinds)
          InkWell(
            onTap: () => setState(() => kinds.contains(k) ? kinds.remove(k) : kinds.add(k)),
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
              decoration: BoxDecoration(border: Border.all(color: kinds.contains(k) ? bone : boneGhost, width: kinds.contains(k) ? 0.9 : 0.5)),
              child: Text(k.toUpperCase(), style: carved(9.5, color: kinds.contains(k) ? bone : boneFaint, spacing: 0.22)),
            ),
          ),
      ]),
      pyrateSwitch(mature, (v) => setState(() => mature = v), 'Mature: strong language or themes. Listeners who hide mature stations will not see it'),
      pyrateSwitch(explicit, (v) => setState(() => explicit = v), 'Explicit: sexual or graphic material. Only listeners who turn explicit stations on will see it'),
      TextField(controller: port, style: serif(16), decoration: pyrateField('Home port (a city, for Local)')),
      TextField(controller: tip, style: mono(12, color: bone), decoration: pyrateField('Tips: a Lightning address, Bitcoin or Monero')),
      const SizedBox(height: 12),
      Row(children: [
        pyrateButton('SET THE PAPERS', () {
          Native.call({'cmd': 'details_set', 'station': widget.station, 'kinds': kinds.toList(), 'mature': mature, 'explicit': explicit});
          final r = Native.call({'cmd': 'colours_set', 'station': widget.station, 'port': port.text, 'tip': tip.text});
          setState(() => saved = r is Map && r['error'] != null ? r['error'] : 'Set. The Log hears it within a minute.');
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
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      heading('ABOARD, HOUR BY HOUR'),
      if (pts.length < 2)
        note('The chart fills in as the hours pass.')
      else ...[
        SizedBox(height: 70, width: double.infinity, child: CustomPaint(painter: _Spark(pts.map((p) => p.$2.toDouble()).toList()))),
        const SizedBox(height: 4),
        Text('most ${pts.map((p) => p.$2).reduce((a, b) => a > b ? a : b)} · now ${pts.last.$2}', style: mono(10)),
      ],
    ]);
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
    final code = TextEditingController();
    final label = TextEditingController();
    final ok = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        backgroundColor: panel,
        shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
        title: Text('ADD A HAND', style: carved(13)),
        content: SingleChildScrollView(
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            note('Your co-host opens Colours, presses JOIN A CREW and sends you the code it shows. Their phone can then broadcast for this station. Your words can take it back at any time.'),
            TextField(controller: code, style: mono(12, color: bone), decoration: pyrateField('Their code')),
            TextField(controller: label, style: serif(16), decoration: pyrateField('Their name, for your own record')),
            TextField(controller: words, style: mono(12, color: bone), maxLines: 3, decoration: pyrateField('Your 24 words')),
          ]),
        ),
        actions: [TextButton(onPressed: () => Navigator.pop(c, true), child: Text('APPOINT', style: carved(11)))],
      ),
    );
    if (ok != true) return;
    final r = await Native.callAway({'cmd': 'hand_appoint', 'station': widget.station, 'words': words.text, 'code': code.text, 'label': label.text});
    setState(() => msg = r is Map && r['error'] != null ? r['error'] : 'Appointed. Tell them your station\'s call sign and id.');
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
        title: Text('JOIN THE CREW', style: carved(13)),
        content: TextField(controller: id, style: mono(12, color: bone), decoration: pyrateField('The station\'s id, from its captain')),
        actions: [TextButton(onPressed: () => Navigator.pop(c, true), child: Text('JOIN', style: carved(11)))],
      ),
    );
    if (ok != true) return;
    final r = Native.call({'cmd': 'hand_join', 'station': id.text.trim()});
    setState(() => msg = r is Map && r['error'] != null ? r['error'] : 'Aboard. The station arrives within a minute.');
  }

  @override
  Widget build(BuildContext context) {
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      heading('HANDS'),
      note('A show with more than one voice. A co-host broadcasts from their own phone; only the captain\'s words can appoint or dismiss them.'),
      const SizedBox(height: 12),
      Wrap(spacing: 10, runSpacing: 10, children: [
        if (widget.station != null) pyrateButton('ADD A HAND', appoint),
        pyrateButton('JOIN A CREW', join),
        if (myCode != null) pyrateButton('I HAVE BEEN APPOINTED', board),
      ]),
      if (widget.station != null)
        Padding(padding: const EdgeInsets.only(top: 10), child: SelectableText('This station\'s id: ${widget.station}', style: mono(10))),
      if (myCode != null) ...[
        const SizedBox(height: 10),
        note('Send this code to the captain:'),
        SelectableText(myCode!, style: mono(11, color: bone)),
        Row(children: [pyrateButton('SEND IT', () => ship.invokeMethod('share_text', {'text': 'My Pyrate Radio hand code: $myCode'}))]),
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
      note('Give hours of the day to chosen recordings: talk in the morning, music after dark. Outside every block the whole library plays. The bell plays one recording on a fixed beat, such as a station ident on the hour, and that recording leaves the ordinary rotation.'),
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
          IconButton(icon: const Icon(Icons.close, size: 16, color: boneGhost), onPressed: () => setState(() => blocks.removeAt(i))),
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
            TextButton(onPressed: () => setState(() => every = m), child: Text('$m MIN', style: carved(10, color: every == m ? bone : boneFaint))),
        ]),
      const SizedBox(height: 8),
      Row(children: [
        pyrateButton('SET THE CLOCK', () {
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

class ShipPapers extends StatefulWidget {
  final Tuner radio;
  const ShipPapers({super.key, required this.radio});
  @override
  State<ShipPapers> createState() => _ShipPapersState();
}

class _ShipPapersState extends State<ShipPapers> {
  bool strict = false;
  bool provisions = true;
  bool plain = false;
  bool plainLock = false;
  bool pinSet = false;
  Map wake = {};
  String? msg;

  @override
  void initState() {
    super.initState();
    strict = (Native.call({'cmd': 'strict'}) as Map?)?['value'] == true;
    provisions = (Native.call({'cmd': 'provisions'}) as Map?)?['value'] != false;
    pinSet = (Native.call({'cmd': 'pin_check', 'pin': ''}) as Map?)?['set'] == true;
    ship.invokeMethod<bool>('plain_get').then((v) => setState(() => plain = v == true)).catchError((_) {});
    ship.invokeMethod<bool>('plain_lock_get').then((v) => setState(() => plainLock = v == true)).catchError((_) {});
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
    setState(() {
      wake = jsonDecode(v ?? '{}');
      msg = exact ? null : 'Android asks permission for alarms that ring on the minute. Without it the radio may wake you a minute late.';
    });
    if (!exact) await ship.invokeMethod('exact_settings');
  }

  Future<void> setPin() async {
    final pin = TextEditingController();
    final ok = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        backgroundColor: panel,
        shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
        title: Text('LOCK COLOURS', style: carved(13)),
        content: TextField(controller: pin, autofocus: true, keyboardType: TextInputType.number, obscureText: true, style: mono(16, color: bone), decoration: pyrateField('Four or more digits')),
        actions: [TextButton(onPressed: () => Navigator.pop(c, true), child: Text('LOCK', style: carved(11)))],
      ),
    );
    if (ok != true || pin.text.length < 4) return;
    Native.call({'cmd': 'pin_set', 'pin': pin.text});
    setState(() => pinSet = true);
  }

  Future<void> importList() async {
    final text = TextEditingController();
    final ok = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        backgroundColor: panel,
        shape: const RoundedRectangleBorder(side: BorderSide(color: boneGhost, width: 0.5)),
        title: Text('TAKE A LIST ABOARD', style: carved(13)),
        content: SingleChildScrollView(
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            note('Paste a maroon list someone shared with you. Every station on it leaves your Log. It is a copy: nobody can change it afterwards.'),
            TextField(controller: text, maxLines: 6, style: mono(11, color: bone), decoration: pyrateField('The list')),
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
          TextButton(onPressed: () => Navigator.pop(c, true), child: Text('MAROON THEM', style: carved(11, color: bloodBright))),
        ],
      ),
    );
    if (ok != true) return;
    final r = Native.call({'cmd': 'maroon_import', 'text': text.text});
    setState(() => msg = r is Map && r['added'] != null ? '${r['added']} stations marooned.' : 'That list would not read.');
  }

  @override
  Widget build(BuildContext context) {
    final wakeOn = wake['on'] == true;
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      heading('WAKE TO THE RADIO'),
      note(wakeOn
          ? 'Every morning at ${(wake['hour'] as num).toInt().toString().padLeft(2, '0')}:${(wake['minute'] as num).toInt().toString().padLeft(2, '0')}, ${wake['name'] ?? 'the station'} rises from quiet. If no sound comes, the phone\'s own alarm rings instead.'
          : 'Wake to a station rising from quiet. If the network is down, the phone\'s own alarm rings instead.'),
      const SizedBox(height: 10),
      Wrap(spacing: 10, runSpacing: 10, children: [
        pyrateButton(wakeOn ? 'CHANGE THE TIME' : 'SET A TIME', setWake),
        if (wakeOn)
          pyrateButton('NO ALARM', () async {
            await ship.invokeMethod('wake_set', {'on': false, 'hour': 7, 'minute': 0});
            setState(() => wake = {'on': false});
          }, color: boneFaint),
      ]),
      const SizedBox(height: 26),
      heading('PASS IT ON'),
      note('Hand the app to someone beside you, with no store and no internet.'),
      const SizedBox(height: 10),
      Row(children: [pyrateButton('PASS IT ON', () => pyrateSheet(context, const PassItOn()))]),
      const SizedBox(height: 26),
      heading('MAROON LISTS'),
      note('Share the stations you have marooned, or take aboard someone else\'s list. Each phone keeps its own copy; nobody can change yours.'),
      const SizedBox(height: 10),
      Wrap(spacing: 10, runSpacing: 10, children: [
        pyrateButton('SHARE MY LIST', () {
          final r = Native.call({'cmd': 'maroon_export'});
          if (r is Map) ship.invokeMethod('share_text', {'text': r['text']});
        }),
        pyrateButton('TAKE A LIST ABOARD', importList),
      ]),
      if (msg != null) Padding(padding: const EdgeInsets.only(top: 8), child: note(msg!)),
      const SizedBox(height: 26),
      heading('PROVISIONS'),
      pyrateSwitch(provisions, (v) {
        Native.call({'cmd': 'provisions', 'value': v});
        setState(() => provisions = v);
      }, 'On Wi-Fi, bring aboard the next six hours of up to three kept stations'),
      const SizedBox(height: 18),
      heading('DISCRETION'),
      pyrateSwitch(plain, (v) {
        ship.invokeMethod('plain_set', {'value': v});
        setState(() => plain = v);
      }, 'A plain icon named Radio in place of the skull'),
      pyrateSwitch(plainLock, (v) {
        ship.invokeMethod('plain_lock_set', {'value': v});
        setState(() => plainLock = v);
      }, 'The lock screen says only "Radio"'),
      pyrateSwitch(pinSet, (v) {
        if (v) {
          setPin();
        } else {
          Native.call({'cmd': 'pin_set'});
          setState(() => pinSet = false);
        }
      }, 'Lock Colours behind a PIN'),
      pyrateSwitch(strict, (v) {
        Native.call({'cmd': 'strict', 'value': v});
        setState(() => strict = v);
      }, 'Never show my address: when a private route fails, wait rather than ask a carrier directly'),
      if (strict) note('Sound may stutter on a thin network, and your own station waits for a stem rather than going on the air from this phone.'),
    ]);
  }
}

/// Asks for the PIN before Colours opens, once per time the app comes to the front.
class PinGate extends StatefulWidget {
  final Widget child;
  const PinGate({super.key, required this.child});
  @override
  State<PinGate> createState() => _PinGateState();
}

class _PinGateState extends State<PinGate> with WidgetsBindingObserver {
  bool open = false;
  final pin = TextEditingController();
  String? wrong;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
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
          Text('COLOURS', style: carved(16)),
          const SizedBox(height: 14),
          TextField(
            controller: pin,
            autofocus: true,
            obscureText: true,
            keyboardType: TextInputType.number,
            textAlign: TextAlign.center,
            style: mono(20, color: bone),
            decoration: pyrateField('PIN'),
            onSubmitted: (_) => check(),
          ),
          if (wrong != null) Padding(padding: const EdgeInsets.only(top: 8), child: Text(wrong!, style: serif(14, color: bloodBright))),
          const SizedBox(height: 14),
          pyrateButton('OPEN', check),
        ]),
      ),
    );
  }

  void check() {
    final ok = (Native.call({'cmd': 'pin_check', 'pin': pin.text}) as Map?)?['ok'] == true;
    setState(() {
      open = ok;
      wrong = ok ? null : 'Not that one.';
      pin.clear();
    });
  }
}
