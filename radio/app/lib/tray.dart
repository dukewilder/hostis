// The radio on a computer with its window closed: an icon in the tray (the menu bar on a Mac),
// the radio carrying for others and playing on if it was playing, until Quit in the icon's
// menu. Its window comes back from the icon. No rights reserved.
import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:tray_manager/tray_manager.dart';
import 'package:window_manager/window_manager.dart';

import 'more.dart' show pref;

class Tray with TrayListener, WindowListener {
  Tray._();
  static final Tray it = Tray._();

  /// Whether the window shows. Hidden, the app asks the node for news less often and nothing
  /// animates, so a radio left running costs the computer next to nothing.
  static final ValueNotifier<bool> visible = ValueNotifier(true);
  static bool get shown => visible.value;
  static set shown(bool v) => visible.value = v;

  /// Where the icon lives, in words.
  static String get place => Platform.isMacOS ? 'the menu bar' : 'the tray';

  /// "Keep the radio running when the window is closed".
  static bool get keep => pref('keep_running') == true;

  bool _icon = false;

  /// Before the first frame. Started with the computer (`--quiet`) and kept running, the radio
  /// starts in the tray with no window.
  Future<void> start({bool hidden = false}) async {
    // PYRATE_TRAY=1 starts it in the tray, kept running, with no window: for measuring what the
    // radio costs a computer left on (tools/test/desk_idle.sh).
    if (Platform.environment['PYRATE_TRAY'] == '1') {
      pref('keep_running', true);
      hidden = true;
    }
    await windowManager.ensureInitialized();
    windowManager.addListener(this);
    trayManager.addListener(this);
    // Every close comes here first, and is a close unless the radio is kept running.
    await windowManager.setPreventClose(true);
    if (keep) await _up();
    if (hidden && keep) {
      shown = false;
      await windowManager.hide();
    }
  }

  /// On or off, from Settings or after the first listen. On puts the icon there at once.
  Future<void> set(bool on) async {
    pref('keep_running', on);
    if (on) {
      await _up();
    } else {
      await _down();
    }
  }

  Future<void> _up() async {
    if (_icon) return;
    _icon = true;
    try {
      await trayManager.setIcon(
        Platform.isWindows ? 'assets/tray/tray.ico' : Platform.isMacOS ? 'assets/tray/tray_mac.png' : 'assets/tray/tray.png',
        isTemplate: Platform.isMacOS,
      );
      if (!Platform.isLinux) await trayManager.setToolTip('Pyrate Radio');
      await trayManager.setContextMenu(Menu(items: [
        MenuItem(key: 'show', label: 'Open Pyrate Radio'),
        MenuItem.separator(),
        MenuItem(key: 'quit', label: 'Quit Pyrate Radio'),
      ]));
    } catch (_) {
      // A desktop with no tray: the window simply closes as it always did.
      _icon = false;
    }
  }

  Future<void> _down() async {
    if (!_icon) return;
    _icon = false;
    await trayManager.destroy().catchError((_) {});
  }

  Future<void> show() async {
    shown = true;
    await windowManager.show();
    await windowManager.focus();
  }

  Future<void> quit() async {
    await _down();
    await windowManager.setPreventClose(false);
    await windowManager.destroy();
  }

  @override
  void onWindowClose() {
    if (keep && _icon) {
      shown = false;
      unawaited(windowManager.hide());
    } else {
      unawaited(quit());
    }
  }

  // Brought back some other way (the Dock on a Mac): news comes at the usual pace again.
  @override
  void onWindowFocus() => shown = true;

  @override
  void onTrayIconMouseDown() => unawaited(show());

  @override
  void onTrayIconRightMouseDown() => unawaited(trayManager.popUpContextMenu());

  @override
  void onTrayMenuItemClick(MenuItem menuItem) {
    if (menuItem.key == 'show') unawaited(show());
    if (menuItem.key == 'quit') unawaited(quit());
  }
}
