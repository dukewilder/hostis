// The app's door to the device. On Android it is the Kotlin shell (MainActivity). On a
// computer there is no shell, so the same calls are answered here: the node in Rust plays,
// goes live and records through the computer's own sound, and the rest is done in Dart or
// left undone where a computer has no such thing (bells, a disguised icon). No rights reserved.

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';

import 'native.dart';

/// The command line a computer started the app with: a station link opened from a browser
/// or a file manager arrives here.
List<String> launchArgs = [];

/// Where the app keeps files for a moment: recordings, shares, imports. On a phone that is
/// the app's own cache. On a computer the shared temporary folder can be read by every
/// account on the machine, so it is a folder of the app's own, emptied at each start.
Future<Directory> scratchDir() async {
  if (Platform.isAndroid || Platform.isIOS) return getTemporaryDirectory();
  final d = Directory('${(await getApplicationSupportDirectory()).path}${Platform.pathSeparator}scratch');
  if (!d.existsSync()) d.createSync(recursive: true);
  return d;
}

/// At start on a computer: empty the scratch folder of anything a crash left, and keep the
/// computer's search index (Spotlight, Windows Search, Tracker) out of the radio's folder.
Future<void> deskTidy(String dir) async {
  if (Platform.isAndroid || Platform.isIOS) return;
  try {
    final s = await scratchDir();
    for (final e in s.listSync()) {
      try { e.deleteSync(recursive: true); } catch (_) {}
    }
    if (Platform.isMacOS) File('$dir/.metadata_never_index').createSync();
    if (Platform.isLinux) File('$dir/.trackerignore').createSync();
    if (Platform.isWindows) await Process.run('attrib', ['+I', dir]);
    if (!Platform.isWindows) await Process.run('chmod', ['700', dir]);
  } catch (_) {}
}

class ShellChannel {
  const ShellChannel();
  static const _android = MethodChannel('pyrate/service');

  Future<T?> invokeMethod<T>(String method, [dynamic arguments]) {
    if (Platform.isAndroid) return _android.invokeMethod<T>(method, arguments);
    return Desktop.call(method, arguments is Map ? arguments : const {}).then((v) => v as T?);
  }
}

class Desktop {
  static String? _link;
  static bool _linkTaken = false;

  /// A copy that leaves the clipboard after a while, if nothing has replaced it, so a link
  /// does not sit in clipboard history or a synced clipboard for days.
  static Future<void> _copy(String text) async {
    await Clipboard.setData(ClipboardData(text: text));
    Timer(const Duration(seconds: 90), () async {
      final now = await Clipboard.getData('text/plain').catchError((_) => null);
      if (now?.text == text) await Clipboard.setData(const ClipboardData(text: ''));
    });
  }

  static Future<dynamic> call(String m, Map a) async {
    switch (m) {
      case 'hold_key':
        return _holdKey();
      case 'scuttle':
        await _scuttle();
        return null;
      case 'build_code':
        // A computer takes its updates from the site, never through the app.
        return 0;
      case 'conditions':
        // A computer is plugged in and on a fixed line or Wi-Fi more often than not.
        return {'charging': true, 'unmetered': true};
      case 'mic':
        return 'ok';
      case 'take_link':
        if (_linkTaken) return null;
        _linkTaken = true;
        return _link ??= _linkFromArgs();
      case 'record_start':
        final dir = await scratchDir();
        final path = '${dir.path}${Platform.pathSeparator}rec-${DateTime.now().millisecondsSinceEpoch}.wav';
        final r = Native.call({'cmd': 'rec_start', 'path': path});
        return r is Map && r['path'] != null ? path : null;
      case 'record_stop':
        final r = Native.call({'cmd': 'rec_stop'});
        return r is Map ? r : {'path': null};
      case 'open_uri':
        return _open(a['uri']?.toString() ?? '');
      case 'play_file':
        return _open(a['path']?.toString() ?? '');
      case 'share_text':
        // A computer has no share sheet: the text goes to the clipboard, ready to paste.
        return _copy(a['text']?.toString() ?? '');
      case 'copy_secret':
        return _copy(a['text']?.toString() ?? '');
      case 'signer':
        return null;
      case 'share_file':
        return _reveal(a['path']?.toString() ?? '');
      case 'share_app':
        return _open('https://hostis.black/pyrateradio/#set');
      case 'speak':
        return false;
      case 'bells':
        return '[]';
      case 'wake_get':
        return '';
      case 'exact_ok':
      case 'plain_get':
      case 'plain_lock_get':
        return m == 'exact_ok';
      case 'pass_start':
        return null;
      default:
        // start, stop, live, secure, lantern, bell_set, bell_cancel, wake_set, remember,
        // app_settings, exact_settings, plain_set, plain_lock_set, pass_stop, install.
        return null;
    }
  }

  static String? _linkFromArgs() {
    for (final a in launchArgs) {
      final m = RegExp(r'#((?:pocket|flock)=\S+|[0-9a-f]{64})').firstMatch(a) ?? RegExp(r'^pyrate://station/([0-9a-f]{64})').firstMatch(a);
      if (m != null) return m.group(1);
    }
    return null;
  }

  static File? _keyFile;
  static Future<File> _key() async => _keyFile ??= File('${(await getApplicationSupportDirectory()).path}${Platform.pathSeparator}hold.key');

  /// A computer has no secure chip the app can reach. The hold key goes to the computer's own
  /// keeper where there is one that needs no prompt: Windows' data protection, bound to the
  /// user's login, or the macOS login keychain. Elsewhere it lives in a file only this user can
  /// read, apart from the node's folder, and the disk's own encryption is what guards it.
  static Future<String> _holdKey() async {
    final hex = RegExp(r'^[0-9a-f]{64}$');
    final f = await _key();
    if (await f.exists()) {
      final t = (await f.readAsString()).trim();
      if (hex.hasMatch(t)) {
        // A key from before, in the clear: moved into the computer's keeper, if it has one.
        final w = await _wrap(t);
        if (w != null) await _write(f, w);
        return t;
      }
      final k = await _unwrap(t);
      if (k != null && hex.hasMatch(k)) return k;
      // A key that cannot be opened is set aside, never overwritten.
      await f.rename('${f.path}.unopened-${DateTime.now().millisecondsSinceEpoch}');
    }
    final r = Random.secure();
    final k = List.generate(32, (_) => r.nextInt(256).toRadixString(16).padLeft(2, '0')).join();
    await f.parent.create(recursive: true);
    await _write(f, await _wrap(k) ?? k);
    return k;
  }

  static Future<void> _write(File f, String text) async {
    await f.writeAsString(text, flush: true);
    if (!Platform.isWindows) await Process.run('chmod', ['600', f.path]);
  }

  static const _kcAccount = 'pyrate-radio', _kcService = 'Pyrate Radio hold key';

  /// The key handed to the computer's keeper, and the marker the file keeps in its place. The
  /// keeper must give the key straight back before the file stops holding it, so a keeper that
  /// takes but cannot return never costs anyone their radio.
  static Future<String?> _wrap(String k) async {
    try {
      String? w;
      if (Platform.isWindows) w = 'dpapi:${base64Encode(Dpapi.protect(utf8.encode(k)))}';
      if (Platform.isMacOS) {
        final r = await Process.run('security', ['add-generic-password', '-U', '-a', _kcAccount, '-s', _kcService, '-w', k]);
        if (r.exitCode == 0) w = 'keychain:$_kcAccount';
      }
      // Linux: the desktop's Secret Service keyring (GNOME Keyring, KWallet), where one runs.
      if (Platform.isLinux) {
        final p = await Process.start('secret-tool', ['store', '--label=$_kcService', 'service', 'pyrate-radio', 'account', _kcAccount]);
        p.stdin.write(k);
        await p.stdin.close();
        if (await p.exitCode == 0) w = 'secret-service:$_kcAccount';
      }
      if (w != null && await _unwrap(w) == k) return w;
    } catch (_) {}
    return null;
  }

  static Future<String?> _unwrap(String t) async {
    try {
      if (t.startsWith('dpapi:') && Platform.isWindows) return utf8.decode(Dpapi.unprotect(base64Decode(t.substring(6))));
      if (t.startsWith('keychain:') && Platform.isMacOS) {
        final r = await Process.run('security', ['find-generic-password', '-a', _kcAccount, '-s', _kcService, '-w']);
        if (r.exitCode == 0) return (r.stdout as String).trim();
      }
      if (t.startsWith('secret-service:') && Platform.isLinux) {
        final r = await Process.run('secret-tool', ['lookup', 'service', 'pyrate-radio', 'account', _kcAccount]);
        if (r.exitCode == 0) return (r.stdout as String).trim();
      }
    } catch (_) {}
    return null;
  }

  /// The key is overwritten and gone, then the app ends: everything the node kept is noise.
  static Future<void> _scuttle() async {
    final f = await _key();
    if (await f.exists()) {
      final r = Random.secure();
      await f.writeAsBytes(List.generate(4096, (_) => r.nextInt(256)), flush: true);
      await f.delete();
    }
    if (Platform.isMacOS) await Process.run('security', ['delete-generic-password', '-a', _kcAccount, '-s', _kcService]).catchError((_) => ProcessResult(0, 0, '', ''));
    if (Platform.isLinux) await Process.run('secret-tool', ['clear', 'service', 'pyrate-radio', 'account', _kcAccount]).catchError((_) => ProcessResult(0, 0, '', ''));
    Timer(const Duration(milliseconds: 300), () => exit(0));
  }

  static Future<bool> _open(String target) async {
    if (target.isEmpty) return false;
    try {
      final r = Platform.isWindows
          ? await Process.run('rundll32', ['url.dll,FileProtocolHandler', target])
          : await Process.run(Platform.isMacOS ? 'open' : 'xdg-open', [target]);
      return r.exitCode == 0;
    } catch (_) {
      return false;
    }
  }

  static Future<bool> _reveal(String path) async {
    if (path.isEmpty) return false;
    try {
      if (Platform.isWindows) return (await Process.run('explorer', ['/select,', path])).exitCode <= 1;
      if (Platform.isMacOS) return (await Process.run('open', ['-R', path])).exitCode == 0;
      return await _open(File(path).parent.path);
    } catch (_) {
      return false;
    }
  }
}
