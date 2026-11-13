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
/// HOSTIS's own stations carry their covers in the app, so a square cover slot shows one
/// before the network answers. Banners still wait for the network.
const bakedArt = {
  'f33eb5c67e9dd58ca8f85aa4c730f23081937ad9e99746829f7233dadff0803f',
  '110243ae29e24f4f77bf4e5b0b9bb115ee4f2a4dfb76d6091fc913c54d97e44a',
  '1243e4ce0ee6a4acbbf471d1e11b0325899d50880a80e5bc22fd65f999864069',
  '8a422de6ab81bc5e3254430c73d13ef820b5b80e4f85bbe33969f30e804b76dd',
  '23e3e7537c08675de0f0bf2ab62cf2ce3f4779625f2e2377c63aef18bb0f417a',
  'dce68389447e6319303ddbc67bb874863559b8f93015bf810a3b18d0b1eab572',
  '1ae4f9701e693ae457d51e386abb935a4fdb36c4dea03e59c2eb96d366ca2ca6',
};

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
          AnimatedOpacity(
              opacity: img == null ? 1 : 0,
              duration: const Duration(milliseconds: 400),
              child: widget.width == widget.height && bakedArt.contains(widget.station) ? Image.asset('assets/stations/${widget.station}.jpg', fit: BoxFit.cover) : Center(child: widget.fallback)),
          AnimatedOpacity(
            opacity: img == null ? 0 : 1,
            duration: const Duration(milliseconds: 400),
            child: img == null ? const SizedBox() : RawImage(image: img, fit: BoxFit.cover, filterQuality: FilterQuality.medium),
          ),
        ]),
      );
}
