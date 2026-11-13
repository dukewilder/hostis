// Pyrate Radio: the dial glass, as the browser draws it. A mark for every station where its
// frequency falls, and names only around the needle: the station in view over its mark and its
// nearest neighbours either side, as many as fit with no two touching, each on a hairline down
// to its mark. Pointing at a mark or touching one names it; dragging carries the needle and the
// names with it, and letting go tunes in. The same with three stations or a thousand: the marks
// only grow denser. No rights reserved.

import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'main.dart';
import 'station.dart' show frequency;

const _low = 87.5, _span = 20.6;

class DialGlass extends StatefulWidget {
  final Tuner radio;
  const DialGlass({super.key, required this.radio});
  @override
  State<DialGlass> createState() => _DialGlassState();
}

class _Name {
  final String id;
  final Rect at;
  final TextPainter text;
  final bool live;
  _Name(this.id, this.at, this.text, this.live);
}

class _DialGlassState extends State<DialGlass> {
  /// Under a pointer or a finger: the station named in the peek, and where a drag has got to.
  String? peek;
  String? dragTo;
  List<_Name> names = const [];

  // The glass is inset so the end numerals and names stay readable.
  double xOf(String id, double w) => w * (0.03 + (frequency(id) - _low) / _span * 0.94);

  List<Map<String, dynamic>> get order {
    final o = [...widget.radio.stations];
    o.sort((a, b) => frequency(a['id'] as String).compareTo(frequency(b['id'] as String)));
    return o;
  }

  String? nearest(double x, double w, List<Map<String, dynamic>> o) {
    if (o.isEmpty) return null;
    var best = o.first['id'] as String;
    for (final s in o) {
      if ((xOf(s['id'] as String, w) - x).abs() < (xOf(best, w) - x).abs()) best = s['id'] as String;
    }
    return best;
  }

  static TextPainter _label(String t, bool on) => TextPainter(
        text: TextSpan(text: t.toUpperCase(), style: caps(10, color: on ? bone : boneFaint, spacing: 0.14)),
        textDirection: TextDirection.ltr,
        maxLines: 1,
        ellipsis: '…',
      )..layout(maxWidth: 140);

  /// The station in view named over its mark, then a name each side in turn, outward, until
  /// neither side has room.
  List<_Name> layout(String? id, double w, List<Map<String, dynamic>> o) {
    final i = o.indexWhere((s) => s['id'] == id);
    if (i < 0 || w <= 0) return const [];
    const gap = 14.0, top = 8.0, h = 22.0;
    String label(Map s) => ((s['name'] ?? '').toString().trim().isEmpty ? s['call_sign'] ?? '' : s['name']).toString();
    _Name make(Map s, double left, TextPainter t) => _Name(s['id'] as String, Rect.fromLTWH(left, top, t.width + (s['live'] == true ? 10 : 0), h), t, s['live'] == true);
    final c = _label(label(o[i]), true);
    final cw = c.width + (o[i]['live'] == true ? 10 : 0);
    double l = (xOf(id!, w) - cw / 2).clamp(0.0, math.max(0.0, w - cw)).toDouble();
    var r = l + cw;
    final out = [make(o[i], l, c)];
    var li = i - 1, ri = i + 1;
    var lOpen = true, rOpen = true;
    while ((lOpen || rOpen) && out.length < 25) {
      if (rOpen) {
        if (ri >= o.length) {
          rOpen = false;
        } else {
          final t = _label(label(o[ri]), false);
          final tw = t.width + (o[ri]['live'] == true ? 10 : 0);
          if (r + gap + tw > w) {
            rOpen = false;
          } else {
            out.add(make(o[ri], r + gap, t));
            r += gap + tw;
            ri++;
          }
        }
      }
      if (lOpen) {
        if (li < 0) {
          lOpen = false;
        } else {
          final t = _label(label(o[li]), false);
          final tw = t.width + (o[li]['live'] == true ? 10 : 0);
          if (l - gap - tw < 0) {
            lOpen = false;
          } else {
            l -= gap + tw;
            out.add(make(o[li], l, t));
            li--;
          }
        }
      }
    }
    return out;
  }

  void tuneTo(String id) {
    if (id != widget.radio.tuned || !widget.radio.playing) widget.radio.tune(id);
  }

  @override
  Widget build(BuildContext context) {
    final radio = widget.radio;
    return LayoutBuilder(builder: (context, box) {
      final w = box.maxWidth - 40;
      final o = order;
      return Semantics(
        label: 'The dial. Drag along it, or tap a name, to tune.',
        child: MouseRegion(
          cursor: SystemMouseCursors.click,
          onHover: (e) => setState(() => peek = nearest(e.localPosition.dx - 20, w, o)),
          onExit: (_) => setState(() => peek = null),
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTapUp: (d) {
              final x = d.localPosition.dx - 20;
              final hit = names.where((n) => n.at.inflate(4).contains(Offset(x, d.localPosition.dy))).firstOrNull;
              final id = hit?.id ?? nearest(x, w, o);
              if (id != null) tuneTo(id);
            },
            onHorizontalDragStart: (d) => setState(() => dragTo = peek = nearest(d.localPosition.dx - 20, w, o)),
            onHorizontalDragUpdate: (d) => setState(() => dragTo = peek = nearest(d.localPosition.dx - 20, w, o)),
            onHorizontalDragEnd: (_) {
              final id = dragTo;
              setState(() => dragTo = peek = null);
              if (id != null) tuneTo(id);
            },
            onHorizontalDragCancel: () => setState(() => dragTo = peek = null),
            child: Container(
              height: 92,
              margin: const EdgeInsets.symmetric(horizontal: 16),
              decoration: BoxDecoration(color: const Color(0xFF080807), border: Border.all(color: line, width: 1)),
              child: AnimatedBuilder(
                // The needle follows the faces as they turn, between their two marks.
                animation: radio.pages,
                builder: (context, _) {
                  final page = radio.pages.hasClients && radio.stations.isNotEmpty ? (radio.pages.page ?? 0.0).clamp(0.0, radio.stations.length - 1.0) : 0.0;
                  final st = radio.stations;
                  String? inView = st.isEmpty ? null : st[page.round().clamp(0, st.length - 1)]['id'] as String;
                  double needle = 0;
                  if (st.isNotEmpty) {
                    final a = page.floor().clamp(0, st.length - 1), b = page.ceil().clamp(0, st.length - 1);
                    final xa = xOf(st[a]['id'] as String, w - 0), xb = xOf(st[b]['id'] as String, w);
                    needle = xa + (xb - xa) * (page - a);
                  }
                  if (dragTo != null) {
                    inView = dragTo;
                    needle = xOf(dragTo!, w);
                  }
                  names = layout(inView, w, o);
                  return Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 3),
                    child: CustomPaint(
                      size: Size(w, 90),
                      painter: _Glass(
                        marks: [for (final s in o) (xOf(s['id'] as String, w), s['live'] == true, s['id'] as String)],
                        names: names,
                        needle: st.isEmpty ? null : needle,
                        inView: inView,
                        peek: peek,
                        peekText: peek == null ? null : _peekText(radio.station(peek)),
                        xOfPeek: peek == null ? 0 : xOf(peek!, w),
                      ),
                    ),
                  );
                },
              ),
            ),
          ),
        ),
      );
    });
  }

  String _peekText(Map<String, dynamic>? s) {
    if (s == null) return '';
    final n = ((s['name'] ?? '').toString().trim().isEmpty ? s['call_sign'] ?? '' : s['name']).toString();
    return '${n.toUpperCase()}  ${frequency(s['id'] as String).toStringAsFixed(3)}';
  }
}

class _Glass extends CustomPainter {
  final List<(double, bool, String)> marks;
  final List<_Name> names;
  final double? needle;
  final String? inView, peek, peekText;
  final double xOfPeek;
  _Glass({required this.marks, required this.names, required this.needle, required this.inView, required this.peek, required this.peekText, required this.xOfPeek});

  @override
  void paint(Canvas canvas, Size size) {
    final w = size.width, h = size.height;
    const scaleH = 30.0, markH = 9.0;
    final base = h - scaleH;
    // The scale: a tick each half megahertz, a longer one each two, a figure each four.
    final tick = Paint()..color = const Color(0x29EDE4CE);
    final major = Paint()..color = boneFaint;
    for (var f = 88.0; f <= 108.0; f += 0.5) {
      final x = w * (0.03 + (f - _low) / _span * 0.94);
      final big = f % 2 == 0;
      canvas.drawRect(Rect.fromLTWH(x, h - (big ? 11 : 5), 1, big ? 11 : 5), big ? major : tick);
      if (f % 4 == 0) {
        final t = TextPainter(text: TextSpan(text: f.toStringAsFixed(0), style: mono(9.5, color: boneFaint)), textDirection: TextDirection.ltr)..layout();
        t.paint(canvas, Offset(x - t.width / 2, h - 13 - t.height));
      }
    }
    // A mark for every station; bone where it is named, red where it is live.
    final named = {for (final n in names) n.id};
    for (final (x, live, id) in marks) {
      final hi = id == peek;
      final c = live ? bloodBright : hi ? bone : named.contains(id) ? boneDim : const Color(0x52EDE4CE);
      canvas.drawRect(Rect.fromLTWH(x, base - (hi ? 12 : markH), 1, hi ? 12 : markH), Paint()..color = c);
    }
    // The names, each on a hairline down to its mark.
    final lead = Paint()
      ..color = const Color(0x33EDE4CE)
      ..strokeWidth = 1;
    final byId = {for (final m in marks) m.$3: m.$1};
    for (final n in names) {
      final mx = byId[n.id];
      if (mx != null && n.id != inView) canvas.drawLine(Offset(n.at.center.dx, n.at.bottom + 2), Offset(mx + 0.5, base - markH), lead);
      var x = n.at.left;
      if (n.live) {
        canvas.drawCircle(Offset(x + 2.5, n.at.center.dy), 2.5, Paint()..color = bloodBright);
        x += 10;
      }
      n.text.paint(canvas, Offset(x, n.at.center.dy - n.text.height / 2));
    }
    // The needle, in blood, with the site's diamond at its head.
    final at = needle;
    if (at != null) {
      final red = Paint()..color = const Color(0xFFC52828);
      canvas.drawRect(Rect.fromLTWH(at, 0, 1, h), red);
      canvas.save();
      canvas.translate(at + 0.5, 0);
      canvas.rotate(math.pi / 4);
      canvas.drawRect(const Rect.fromLTWH(-3.5, -3.5, 7, 7), red);
      canvas.restore();
    }
    // What a pointer or a finger is on, named over its mark.
    final pt = peekText;
    if (pt != null && pt.isNotEmpty) {
      final t = TextPainter(text: TextSpan(text: pt, style: caps(10, color: bone, spacing: 0.12)), textDirection: TextDirection.ltr, maxLines: 1, ellipsis: '…')..layout(maxWidth: w - 8);
      final bw = t.width + 14;
      final left = (xOfPeek - bw / 2).clamp(-2.0, w - bw + 2);
      final r = Rect.fromLTWH(left, base - markH - 26, bw, 20);
      canvas.drawRect(r, Paint()..color = black);
      canvas.drawRect(r, Paint()
        ..color = boneGhost
        ..style = PaintingStyle.stroke);
      t.paint(canvas, Offset(r.left + 7, r.center.dy - t.height / 2));
    }
  }

  @override
  bool shouldRepaint(_Glass old) => true;
}
