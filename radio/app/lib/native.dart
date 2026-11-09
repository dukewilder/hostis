// The door into the node. Three C functions, JSON across them.
// No rights reserved.

import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';

import 'package:ffi/ffi.dart';

typedef _StartC = Int32 Function(Pointer<Utf8>, Pointer<Utf8>);
typedef _StartD = int Function(Pointer<Utf8>, Pointer<Utf8>);
typedef _CallC = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _CallD = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _FreeC = Void Function(Pointer<Utf8>);
typedef _FreeD = void Function(Pointer<Utf8>);

DynamicLibrary _open() {
  if (Platform.isWindows) return DynamicLibrary.open('pyrate_ffi.dll');
  if (Platform.isMacOS) return DynamicLibrary.open('libpyrate_ffi.dylib');
  return DynamicLibrary.open('libpyrate_ffi.so');
}

class Native {
  static final DynamicLibrary _lib = _open();
  static final _StartD _start = _lib.lookupFunction<_StartC, _StartD>('pyrate_start');
  static final _CallD _call = _lib.lookupFunction<_CallC, _CallD>('pyrate_call');
  static final _FreeD _free = _lib.lookupFunction<_FreeC, _FreeD>('pyrate_free');

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

  /// For calls that work hard (raising colours does its proof of work).
  static Future<dynamic> callAway(Map<String, dynamic> req) => Isolate.run(() => Native.call(req));
}
