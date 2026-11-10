// The app's door to the device. On Android it is the Kotlin shell (MainActivity). On a
// computer there is no shell, so the same calls are answered here: the node in Rust plays,
// goes live and records through the computer's own sound, and the rest is done in Dart or
// left undone where a computer has no such thing (bells, a disguised icon). No rights reserved.

import 'dart:async';
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

  /// A computer has no secure chip the app can reach, so the hold key lives in a file only this
  /// user can read, apart from the node's own folder. A copy of the node's folder alone is noise.
  static Future<String> _holdKey() async {
    final f = await _key();
    if (await f.exists()) {
      final k = (await f.readAsString()).trim();
      if (RegExp(r'^[0-9a-f]{64}$').hasMatch(k)) return k;
    }
    final r = Random.secure();
    final k = List.generate(32, (_) => r.nextInt(256).toRadixString(16).padLeft(2, '0')).join();
    await f.parent.create(recursive: true);
    await f.writeAsString(k, flush: true);
    if (!Platform.isWindows) await Process.run('chmod', ['600', f.path]);
    return k;
  }

  /// The key is overwritten and gone, then the app ends: everything the node kept is noise.
  static Future<void> _scuttle() async {
    final f = await _key();
    if (await f.exists()) {
      final r = Random.secure();
      await f.writeAsBytes(List.generate(4096, (_) => r.nextInt(256)), flush: true);
      await f.delete();
    }
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
