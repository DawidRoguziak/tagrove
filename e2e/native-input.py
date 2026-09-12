"""Native input for an explicitly selected private E2E display, including a Wayland viewer."""
import ctypes as c
import os
import sys
import time

assert os.environ["DISPLAY"] == os.environ["MEDIATAGGER_NATIVE_DISPLAY"]
assert os.environ["DISPLAY"] not in (":0", ":1")
assert os.environ["XDG_DATA_HOME"].startswith("/tmp/mediatagger-")
x = c.CDLL("libX11.so.6")
t = c.CDLL("libXtst.so.6")
x.XOpenDisplay.restype = c.c_void_p
x.XOpenDisplay.argtypes = [c.c_char_p]
d = x.XOpenDisplay(None)
assert d
x.XFlush.argtypes = [c.c_void_p]
x.XCloseDisplay.argtypes = [c.c_void_p]
t.XTestFakeMotionEvent.argtypes = [c.c_void_p, c.c_int, c.c_int, c.c_int, c.c_ulong]
t.XTestFakeButtonEvent.argtypes = [c.c_void_p, c.c_uint, c.c_int, c.c_ulong]
t.XTestFakeKeyEvent.argtypes = [c.c_void_p, c.c_uint, c.c_int, c.c_ulong]
x.XStringToKeysym.argtypes = [c.c_char_p]
x.XStringToKeysym.restype = c.c_ulong
x.XKeysymToKeycode.argtypes = [c.c_void_p, c.c_ulong]
x.XKeysymToKeycode.restype = c.c_uint
if sys.argv[1] == "focus-away":
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gtk, GLib
    window = Gtk.Window(title="Native video focus fixture")
    window.set_default_size(160, 100)
    window.show_all()
    window.present()
    if os.environ["GDK_BACKEND"] == "x11":
        gi.require_version("GdkX11", "3.0")
        from gi.repository import GdkX11
        x.XSetInputFocus.argtypes = [c.c_void_p, c.c_ulong, c.c_int, c.c_ulong]
        x.XSetInputFocus(d, window.get_window().get_xid(), 1, 0)
        x.XFlush(d)
    focused = []
    def finish():
        focused.append(window.is_active())
        window.destroy()
        Gtk.main_quit()
        return False
    GLib.timeout_add(700, finish)
    Gtk.main()
    assert focused == [True], "Focus fixture never became active"
elif sys.argv[1] == "key":
    code = x.XKeysymToKeycode(d, x.XStringToKeysym(sys.argv[2].encode()))
    t.XTestFakeKeyEvent(d, code, 1, 0)
    t.XTestFakeKeyEvent(d, code, 0, 50)
else:
    assert sys.argv[1] in ("click", "move", "drag", "focus")
    t.XTestFakeMotionEvent(d, -1, int(sys.argv[2]), int(sys.argv[3]), 0)
    if sys.argv[1] == "focus":
        # Focus the application window under the supplied point on the owned display.
        x.XDefaultRootWindow.argtypes = [c.c_void_p]
        x.XDefaultRootWindow.restype = c.c_ulong
        x.XQueryPointer.argtypes = [c.c_void_p, c.c_ulong, c.POINTER(c.c_ulong), c.POINTER(c.c_ulong),
                                   c.POINTER(c.c_int), c.POINTER(c.c_int), c.POINTER(c.c_int), c.POINTER(c.c_int), c.POINTER(c.c_uint)]
        x.XSetInputFocus.argtypes = [c.c_void_p, c.c_ulong, c.c_int, c.c_ulong]
        root, child = c.c_ulong(), c.c_ulong()
        rx, ry, wx, wy, mask = c.c_int(), c.c_int(), c.c_int(), c.c_int(), c.c_uint()
        x.XFlush(d)
        assert x.XQueryPointer(d, x.XDefaultRootWindow(d), c.byref(root), c.byref(child),
                               c.byref(rx), c.byref(ry), c.byref(wx), c.byref(wy), c.byref(mask))
        assert child.value, "No application window at the focus point"
        x.XSetInputFocus(d, child, 1, 0)
        t.XTestFakeButtonEvent(d, 1, 1, 0)
        t.XTestFakeButtonEvent(d, 1, 0, 80)
    elif sys.argv[1] != "move":
        t.XTestFakeButtonEvent(d, 1, 1, 0)
        if sys.argv[1] == "drag":
            t.XTestFakeMotionEvent(d, -1, int(sys.argv[4]), int(sys.argv[5]), 80)
        t.XTestFakeButtonEvent(d, 1, 0, 80)
x.XFlush(d)
time.sleep(.2)
x.XCloseDisplay(d)
