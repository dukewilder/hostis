#!/usr/bin/python3.12
"""A stand-in for the desktop's file chooser (xdg-desktop-portal) on a test session bus, so the
app's own pickers (recordings, pictures, music for the desk) can be driven under Xvfb.

OpenFile and SaveFile answer with the paths listed in $PORTAL_PICK, one per line, as if the
user had chosen them; the list is emptied once read, and an empty list answers as a dialog the
user cancelled. Every call is written to $PORTAL_LOG as a line of JSON. Needs the system's
Python 3.12 (dbus-python and GLib bindings). Started by app_sweep.py; nothing else uses it."""
import os, json, urllib.parse
import dbus, dbus.service, dbus.mainloop.glib
from gi.repository import GLib

dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
PICK, LOG = os.environ["PORTAL_PICK"], os.environ.get("PORTAL_LOG", "/dev/null")


class Request(dbus.service.Object):
    @dbus.service.signal("org.freedesktop.portal.Request", signature="ua{sv}")
    def Response(self, response, results):
        pass

    @dbus.service.method("org.freedesktop.portal.Request")
    def Close(self):
        pass


class Chooser(dbus.service.Object):
    n = 0

    @dbus.service.method("org.freedesktop.portal.FileChooser", in_signature="ssa{sv}", out_signature="o", sender_keyword="sender")
    def OpenFile(self, parent, title, options, sender=None):
        return self.answer("OpenFile", title, options, sender)

    @dbus.service.method("org.freedesktop.portal.FileChooser", in_signature="ssa{sv}", out_signature="o", sender_keyword="sender")
    def SaveFile(self, parent, title, options, sender=None):
        return self.answer("SaveFile", title, options, sender)

    def answer(self, kind, title, options, sender):
        Chooser.n += 1
        token = str(options.get("handle_token", "t"))
        path = f"/org/freedesktop/portal/desktop/request/{(sender or ':0').lstrip(':').replace('.', '_')}/{token}{Chooser.n}"
        req = Request(bus, path)
        try:
            paths = [l.strip() for l in open(PICK) if l.strip()]
        except FileNotFoundError:
            paths = []
        open(PICK, "w").close()
        with open(LOG, "a") as f:
            f.write(json.dumps({"kind": kind, "title": str(title), "multiple": bool(options.get("multiple", False)), "picked": paths}) + "\n")

        # The app listens for the answer once the call returns, so it comes a moment later.
        def respond():
            if paths:
                req.Response(dbus.UInt32(0), dbus.Dictionary({"uris": dbus.Array(["file://" + urllib.parse.quote(p) for p in paths], signature="s")}, signature="sv"))
            else:
                req.Response(dbus.UInt32(1), dbus.Dictionary({}, signature="sv"))
            return False

        GLib.timeout_add(400, respond)
        return dbus.ObjectPath(path)


name = dbus.service.BusName("org.freedesktop.portal.Desktop", bus)
Chooser(bus, "/org/freedesktop/portal/desktop")
GLib.MainLoop().run()
