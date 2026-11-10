import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    // A radio's shape: tall and narrow, black from the first frame.
    var windowFrame = self.frame
    windowFrame.size = NSSize(width: 460, height: 900)
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)
    self.title = "Pyrate Radio"
    self.backgroundColor = NSColor(red: 3/255, green: 3/255, blue: 3/255, alpha: 1)
    self.minSize = NSSize(width: 380, height: 640)

    RegisterGeneratedPlugins(registry: flutterViewController)

    super.awakeFromNib()
  }
}
