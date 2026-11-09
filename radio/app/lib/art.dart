// Station art. Pictures come from the node as pixels; browsing never writes them to disk.
// No rights reserved.

import 'dart:async';
import 'dart:collection';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';

import 'native.dart';

/// Decoded pictures, newest kept. The node keeps its own shelf; this one saves decoding twice.
class ArtShelf {
  static final _items = LinkedHashMap<String, ui.Image>();
  static ui.Image? get(String h) {
    final i = _items.remove(h);
    if (i != null) _items[h] = i;
    return i;
  }

  static void put(String h, ui.Image i) {
    _items[h] = i;
    while (_items.length > 48) {
      _items.remove(_items.keys.first);
    }
  }
}

Color? accentOf(dynamic hex) {
  if (hex is! String || hex.length != 7) return null;
  final v = int.tryParse(hex.substring(1), radix: 16);
  return v == null ? null : Color(0xFF000000 | v);
}

/// A station's picture, drawn over [fallback] and faded in once it arrives.
class StationArt extends StatefulWidget {
  final String station;
  final String? hash;
  final double width;
  final double height;
  final Widget fallback;
  const StationArt({super.key, required this.station, required this.hash, required this.width, required this.height, required this.fallback});

  @override
  State<StationArt> createState() => _StationArtState();
}

class _StationArtState extends State<StationArt> {
  ui.Image? img;
  Timer? _wait;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void didUpdateWidget(StationArt old) {
    super.didUpdateWidget(old);
    if (old.hash != widget.hash) {
      img = null;
      _load();
    }
  }

  void _load() {
    _wait?.cancel();
    final h = widget.hash;
    if (h == null) return;
    final have = ArtShelf.get(h);
    if (have != null) {
      img = have;
      return;
    }
    Native.call({'cmd': 'art_want', 'station': widget.station, 'hash': h});
    var tries = 0;
    _wait = Timer.periodic(const Duration(milliseconds: 400), (t) async {
      if (++tries > 60) return t.cancel();
      final i = await Native.art(h);
      if (i == null || !mounted || widget.hash != h) return;
      t.cancel();
      ArtShelf.put(h, i);
      setState(() => img = i);
    });
  }

  @override
  void dispose() {
    _wait?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => SizedBox(
        width: widget.width,
        height: widget.height,
        child: Stack(fit: StackFit.expand, children: [
          AnimatedOpacity(opacity: img == null ? 1 : 0, duration: const Duration(milliseconds: 400), child: Center(child: widget.fallback)),
          AnimatedOpacity(
            opacity: img == null ? 0 : 1,
            duration: const Duration(milliseconds: 400),
            child: img == null ? const SizedBox() : RawImage(image: img, fit: BoxFit.cover, filterQuality: FilterQuality.medium),
          ),
        ]),
      );
}
