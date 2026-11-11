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
import 'package:flutter/services.dart' show Clipboard, ClipboardData;

import 'shell.dart';

const _shell = ShellChannel();

/// While the 24 words or a PIN are on screen: no screenshot, no recents thumbnail, no
/// screen recording.
Future<void> secure(bool on) => _shell.invokeMethod('secure', {'on': on}).catchError((_) {});

/// A copy marked sensitive, so keyboards and clipboard previews keep it out of their
/// history and their cloud. It clears itself after a minute and a half if still there.
Future<void> copySecret(String text) => _shell.invokeMethod('copy_secret', {'text': text}).catchError((_) => Clipboard.setData(ClipboardData(text: text)));

/// The SHA-256 of the certificate this copy of the app was signed with, or null.
Future<String?> signer() => _shell.invokeMethod<String>('signer').catchError((_) => null);

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

// ---------------------------------------------------------------------------------------
// Windows: the hold key under DPAPI, bound to the user's own login

final class _Blob extends Struct {
  @Uint32()
  external int cbData;
  external Pointer<Uint8> pbData;
}

typedef _CryptC = Int32 Function(Pointer<_Blob>, Pointer<Void>, Pointer<_Blob>, Pointer<Void>, Pointer<Void>, Uint32, Pointer<_Blob>);
typedef _CryptD = int Function(Pointer<_Blob>, Pointer<Void>, Pointer<_Blob>, Pointer<Void>, Pointer<Void>, int, Pointer<_Blob>);
typedef _LocalFree = Pointer<Void> Function(Pointer<Void>);

/// Windows' own data protection: what it seals opens only for the same user on the same
/// computer, with that user's password behind it. A copied disk gives up nothing.
class Dpapi {
  static List<int> protect(List<int> data) => _run('CryptProtectData', data);
  static List<int> unprotect(List<int> data) => _run('CryptUnprotectData', data);

  static List<int> _run(String fn, List<int> data) {
    final f = DynamicLibrary.open('crypt32.dll').lookupFunction<_CryptC, _CryptD>(fn);
    final free = DynamicLibrary.open('kernel32.dll').lookupFunction<_LocalFree, _LocalFree>('LocalFree');
    final input = calloc<_Blob>();
    final out = calloc<_Blob>();
    final buf = calloc<Uint8>(data.length);
    try {
      buf.asTypedList(data.length).setAll(0, data);
      input.ref.cbData = data.length;
      input.ref.pbData = buf;
      // 1: CRYPTPROTECT_UI_FORBIDDEN, never a prompt.
      if (f(input, nullptr, nullptr, nullptr, nullptr, 1, out) == 0) throw StateError('$fn failed');
      final r = List<int>.from(out.ref.pbData.asTypedList(out.ref.cbData));
      out.ref.pbData.asTypedList(out.ref.cbData).fillRange(0, out.ref.cbData, 0);
      free(out.ref.pbData.cast());
      return r;
    } finally {
      buf.asTypedList(data.length).fillRange(0, data.length, 0);
      calloc.free(buf);
      calloc.free(input);
      calloc.free(out);
    }
  }
}
