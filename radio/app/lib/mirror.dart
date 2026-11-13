// The open player, carried inside the computers' app so it can be had where hostis.black
// cannot: served on this Wi-Fi to anyone beside you, or saved to a folder to pass on by USB.
// It is the player of this release, so each update of the app brings its own.
// No rights reserved.

import 'dart:io';

import 'package:flutter/services.dart';

/// The port the player is shared on, the same on every computer so a link stays good.
const playerPort = 47824;

class OpenPlayer {
  static Map<String, Uint8List>? _files;
  static HttpServer? _server;

  /// The player's files, from the app's own copy. Null in a build that carries none.
  static Future<Map<String, Uint8List>?> files() async {
    if (_files == null) {
      try {
        final data = await rootBundle.load('assets/player/open-player.tar');
        _files = untar(data.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes));
      } catch (_) {
        _files = {};
      }
    }
    return _files!.containsKey('index.html') ? _files : null;
  }

  static bool get sharing => _server != null;

  /// Starts or stops sharing on every network this computer is on. Null when done, otherwise
  /// why not.
  static Future<String?> share(bool on) async {
    if (!on) {
      await _server?.close(force: true);
      _server = null;
      return null;
    }
    if (_server != null) return null;
    final f = await files();
    if (f == null) return 'this build carries no player';
    try {
      _server = await HttpServer.bind(InternetAddress.anyIPv4, playerPort);
    } catch (_) {
      return 'port $playerPort is in use';
    }
    _server!.listen((r) => _answer(r, f), onError: (_) {});
    return null;
  }

  static Future<void> _answer(HttpRequest r, Map<String, Uint8List> f) async {
    final res = r.response;
    try {
      var path = r.uri.path.startsWith('/') ? r.uri.path.substring(1) : r.uri.path;
      if (path.isEmpty || path.endsWith('/')) path = '${path}index.html';
      final body = f[path];
      if ((r.method != 'GET' && r.method != 'HEAD') || body == null) {
        res.statusCode = HttpStatus.notFound;
      } else {
        final ext = path.contains('.') ? path.substring(path.lastIndexOf('.') + 1) : '';
        res.headers
          ..set(HttpHeaders.contentTypeHeader, contentType(ext))
          ..set(HttpHeaders.cacheControlHeader, ext == 'html' || ext == 'webmanifest' ? 'no-cache' : 'max-age=600')
          ..set('X-Content-Type-Options', 'nosniff')
          ..set('Referrer-Policy', 'no-referrer')
          ..set('Content-Security-Policy', "frame-ancestors 'self'");
        res.contentLength = body.length;
        if (r.method == 'GET') res.add(body);
      }
    } catch (_) {}
    await res.close().catchError((_) {});
  }

  /// The address someone on the same Wi-Fi opens: this computer's own on the local network.
  static Future<String?> lanUrl() async {
    final found = <String>[];
    try {
      for (final i in await NetworkInterface.list(type: InternetAddressType.IPv4)) {
        for (final a in i.addresses) {
          found.add(a.address);
        }
      }
    } catch (_) {}
    // A home or hotspot address first; never loopback or a link-local one nobody else can reach.
    int rank(String a) => a.startsWith('127.') || a.startsWith('169.254.')
        ? 9
        : a.startsWith('192.168.')
            ? 0
            : a.startsWith('10.')
                ? 1
                : RegExp(r'^172\.(1[6-9]|2\d|3[01])\.').hasMatch(a)
                    ? 2
                    : 5;
    found.sort((a, b) => rank(a).compareTo(rank(b)));
    final ip = found.where((a) => rank(a) < 9).firstOrNull;
    return ip == null ? null : 'http://$ip:$playerPort/';
  }

  /// Writes the player into a folder of its own inside [dir], with a short README. The number of
  /// files written, or null when this build carries no player.
  static Future<int?> save(String dir) async {
    final f = await files();
    if (f == null) return null;
    final root = Directory('$dir${Platform.pathSeparator}Pyrate Radio player');
    for (final e in f.entries) {
      final out = File('${root.path}${Platform.pathSeparator}${e.key.split('/').join(Platform.pathSeparator)}');
      await out.parent.create(recursive: true);
      await out.writeAsBytes(e.value);
    }
    await File('${root.path}${Platform.pathSeparator}README.txt').writeAsString(readme);
    return f.length;
  }

  static const readme = '''Pyrate Radio: the open player

The browser radio that plays every station on the Log and raises new ones. It needs no
hostis.black: it finds the network by itself. No rights reserved.

A browser will not run it straight from the folder. Serve the folder, then open the address:

  On any computer with Python:   python3 -m http.server 8080   (in this folder)
                                 then open http://localhost:8080/
  On any web host:               upload the folder as it is
  On a Lantern:                  lantern --http 0.0.0.0:8080 --player <this folder>
                                 then open http://<its address>:8080/listen/
  On IPFS:                       ipfs add -r <this folder>
  In the Pyrate Radio app:       Settings, Share the player on this network

Pass the folder on as it is. Every copy is one more that no takedown can close.
''';
}

String contentType(String ext) => switch (ext) {
      'html' => 'text/html; charset=utf-8',
      'js' || 'mjs' => 'text/javascript; charset=utf-8',
      'wasm' => 'application/wasm',
      'json' => 'application/json',
      'webmanifest' => 'application/manifest+json',
      'png' => 'image/png',
      'jpg' || 'jpeg' => 'image/jpeg',
      'webp' => 'image/webp',
      'css' => 'text/css; charset=utf-8',
      'svg' => 'image/svg+xml',
      'woff2' => 'font/woff2',
      'txt' => 'text/plain; charset=utf-8',
      _ => 'application/octet-stream',
    };

/// The files in a plain tar (as tools/open_player.py writes it): name to bytes. Only regular
/// files with plain relative names are taken.
Map<String, Uint8List> untar(Uint8List t) {
  final out = <String, Uint8List>{};
  String field(int at, int len) {
    var end = at;
    while (end < at + len && t[end] != 0) {
      end++;
    }
    return String.fromCharCodes(t, at, end);
  }

  var i = 0;
  while (i + 512 <= t.length && t[i] != 0) {
    final prefix = field(i + 345, 155);
    final name = prefix.isEmpty ? field(i + 0, 100) : '$prefix/${field(i + 0, 100)}';
    final size = int.tryParse(field(i + 124, 12).trim(), radix: 8) ?? 0;
    final kind = t[i + 156];
    final start = i + 512;
    if (start + size > t.length) break;
    final plain = name.split('/').every((s) => s.isNotEmpty && !s.startsWith('.') && !s.contains('\\') && !s.contains(':'));
    if ((kind == 0x30 || kind == 0) && plain) out[name] = Uint8List.sublistView(t, start, start + size);
    i = start + (size + 511) ~/ 512 * 512;
  }
  return out;
}
