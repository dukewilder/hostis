// The door into the node. Three C functions, JSON across them.
// No rights reserved.

import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:ffi/ffi.dart';
import 'shell.dart';

const _shell = ShellChannel();

/// While the 24 words or a PIN are on screen: no screenshot, no recents thumbnail, no
/// screen recording.
Future<void> secure(bool on) => _shell.invokeMethod('secure', {'on': on}).catchError((_) {});

typedef _StartC = Int32 Function(Pointer<Utf8>, Pointer<Utf8>);
typedef _StartD = int Function(Pointer<Utf8>, Pointer<Utf8>);
typedef _CallC = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _CallD = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _FreeC = Void Function(Pointer<Utf8>);
typedef _FreeD = void Function(Pointer<Utf8>);

DynamicLibrary _open() {
  // A computer's build carries the node beside the app: next to the program on Windows, in
  // the bundle's Frameworks on a Mac, in the bundle's lib on Linux. Android packs it itself.
  final here = File(Platform.resolvedExecutable).parent.path;
  if (Platform.isWindows) return DynamicLibrary.open('$here\\pyrate_ffi.dll');
  if (Platform.isMacOS) return DynamicLibrary.open('$here/../Frameworks/libpyrate_ffi.dylib');
  if (Platform.isLinux) return DynamicLibrary.open('$here/lib/libpyrate_ffi.so');
  return DynamicLibrary.open('libpyrate_ffi.so');
}

class Native {
  static final DynamicLibrary _lib = _open();
  static final _StartD _start = _lib.lookupFunction<_StartC, _StartD>('pyrate_start');
  static final _CallD _call = _lib.lookupFunction<_CallC, _CallD>('pyrate_call');
  static final _FreeD _free = _lib.lookupFunction<_FreeC, _FreeD>('pyrate_free');

  /// The device's hold key, unwrapped by the phone's secure hardware. Before `start`.
  static void setHoldKey(String hex) {
    final f = _lib.lookupFunction<Void Function(Pointer<Utf8>), void Function(Pointer<Utf8>)>('pyrate_set_hold_key');
    final p = hex.toNativeUtf8();
    f(p);
    calloc.free(p);
  }

  static void setReleaseKeys(String keys) {
    final f = _lib.lookupFunction<Void Function(Pointer<Utf8>), void Function(Pointer<Utf8>)>('pyrate_set_release_keys');
    final p = keys.toNativeUtf8();
    f(p);
    calloc.free(p);
  }

  static int start(String dir, String bootstrap) {
    final d = dir.toNativeUtf8();
    final b = bootstrap.toNativeUtf8();
    try {
      return _start(d, b);
    } finally {
      calloc.free(d);
      calloc.free(b);
    }
  }

  static dynamic call(Map<String, dynamic> req) {
    final p = jsonEncode(req).toNativeUtf8();
    try {
      final r = _call(p);
      final s = r.toDartString();
      _free(r);
      return jsonDecode(s);
    } finally {
      calloc.free(p);
    }
  }

  /// A picture the node holds, decoded by the node in memory-safe code and handed over as
  /// raw pixels, so the phone's own image decoder never sees a stranger's file.
  static Future<ui.Image?> art(String hash) async {
    final f = _lib.lookupFunction<Pointer<Uint8> Function(Pointer<Utf8>, Pointer<Size>), Pointer<Uint8> Function(Pointer<Utf8>, Pointer<Size>)>('pyrate_art');
    final freeBytes = _lib.lookupFunction<Void Function(Pointer<Uint8>, Size), void Function(Pointer<Uint8>, int)>('pyrate_free_bytes');
    final h = hash.toNativeUtf8();
    final len = calloc<Size>();
    try {
      final p = f(h, len);
      if (p == nullptr) return null;
      final n = len.value;
      final bytes = Uint8List.fromList(p.asTypedList(n));
      freeBytes(p, n);
      final hd = ByteData.sublistView(bytes, 0, 8);
      final w = hd.getUint32(0, Endian.little), ht = hd.getUint32(4, Endian.little);
      final c = Completer<ui.Image>();
      ui.decodeImageFromPixels(Uint8List.sublistView(bytes, 8), w, ht, ui.PixelFormat.rgba8888, c.complete);
      return await c.future;
    } finally {
      calloc.free(h);
      calloc.free(len);
    }
  }

  /// For calls that work hard (raising colours does its proof of work).
  static Future<dynamic> callAway(Map<String, dynamic> req) => Isolate.run(() => Native.call(req));
}
