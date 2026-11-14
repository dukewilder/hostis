import Cocoa
import FlutterMacOS

@main
class AppDelegate: FlutterAppDelegate {
  // The radio may keep running with its window closed, in the menu bar (lib/tray.dart): it
  // quits from there, or when the window closes with that setting off.
  override func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    return false
  }

  // A click on the Dock icon brings the window back.
  override func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
    if !flag {
      for window in sender.windows {
        window.makeKeyAndOrderFront(self)
      }
    }
    return true
  }

  override func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
    return true
  }
}
