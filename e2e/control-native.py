"""Native X input for one app:control session's private display.

Spawned only by e2e/control.js with an environment built from the session manifest
(control-support.js nativeEnvironment). Every check runs before libX11 is loaded, so a
refused call never touches any display. `--check` validates and exits without X.

Usage: control-native.py [--check] key <keysym[+keysym...]>
                                   type <printable ASCII>
                                   click <x> <y> [button 1-3]
                                   move <x> <y>
                                   drag <x1> <y1> <x2> <y2>
                                   wheel <x> <y> <steps -50..50, not 0> [ctrl]
"""
import os
import re
import stat
import sys
import time

SCREEN_WIDTH, SCREEN_HEIGHT = 1440, 900
KEYSYM = re.compile(r"^[A-Za-z0-9_]+(\+[A-Za-z0-9_]+){0,3}$")
SESSION_ID = re.compile(r"^[a-f0-9]{32}$")
DISPLAY = re.compile(r"^:\d+$")


class Refused(Exception):
    pass


def require(condition, message):
    if not condition:
        raise Refused(message)


def validate_environment(env):
    display = env.get("DISPLAY", "")
    require(DISPLAY.match(display), "DISPLAY is not a local numbered display")
    require(display not in (":0", ":1"), "refusing the default user displays :0 and :1")
    require(env.get("MEDIATAGGER_NATIVE_DISPLAY") == display, "DISPLAY differs from MEDIATAGGER_NATIVE_DISPLAY")
    scratch = env.get("MEDIATAGGER_CONTROL_SCRATCH", "")
    require(os.path.isabs(scratch), "MEDIATAGGER_CONTROL_SCRATCH must be absolute")
    require(os.path.basename(scratch).startswith("media-tagger-control-"), "scratch is not a controller root")
    info = os.lstat(scratch)
    require(stat.S_ISDIR(info.st_mode) and not stat.S_ISLNK(info.st_mode), "scratch is not a real directory")
    require(os.path.realpath(scratch) == scratch, "scratch is not a canonical path")
    require(env.get("XAUTHORITY") == os.path.join(scratch, "Xauthority"), "XAUTHORITY is not the session's")
    require(env.get("XDG_DATA_HOME") == os.path.join(scratch, "data"), "XDG_DATA_HOME is not the session's")
    session = env.get("MEDIATAGGER_CONTROL_ID", "")
    require(SESSION_ID.match(session), "MEDIATAGGER_CONTROL_ID is not a session ID")
    with open(os.path.join(scratch, "owner"), encoding="utf-8") as owner:
        require(owner.read() == session, "scratch owner differs from the session ID")


def integer(text, low, high, name):
    require(re.fullmatch(r"[+-]?\d{1,7}", text or "") is not None, f"{name} must be an integer")
    value = int(text)
    require(low <= value <= high, f"{name} must be within {low}..{high}")
    return value


def validate_arguments(argv):
    require(argv, "missing native subcommand")
    sub, rest = argv[0], argv[1:]
    x = lambda text: integer(text, 0, SCREEN_WIDTH - 1, "x")  # noqa: E731
    y = lambda text: integer(text, 0, SCREEN_HEIGHT - 1, "y")  # noqa: E731
    if sub == "key":
        require(len(rest) == 1 and KEYSYM.match(rest[0]), "key expects one keysym such as Return or Control+a")
        return sub, [rest[0]]
    if sub == "type":
        require(len(rest) == 1 and 0 < len(rest[0]) <= 1000, "type expects 1..1000 characters")
        require(all(0x20 <= ord(ch) <= 0x7E for ch in rest[0]), "type accepts printable ASCII only")
        return sub, [rest[0]]
    if sub == "click":
        require(len(rest) in (2, 3), "click <x> <y> [button]")
        return sub, [x(rest[0]), y(rest[1]), integer(rest[2], 1, 3, "button") if len(rest) == 3 else 1]
    if sub == "move":
        require(len(rest) == 2, "move <x> <y>")
        return sub, [x(rest[0]), y(rest[1])]
    if sub == "drag":
        require(len(rest) == 4, "drag <x1> <y1> <x2> <y2>")
        return sub, [x(rest[0]), y(rest[1]), x(rest[2]), y(rest[3])]
    if sub == "wheel":
        require(len(rest) in (3, 4), "wheel <x> <y> <steps> [ctrl]")
        steps = integer(rest[2], -50, 50, "steps")
        require(steps != 0, "steps must not be 0")
        require(len(rest) == 3 or rest[3] == "ctrl", "the optional wheel flag is ctrl")
        return sub, [x(rest[0]), y(rest[1]), steps, len(rest) == 4]
    raise Refused("subcommand must be key, type, click, move, drag, or wheel")


def perform(sub, values):
    import ctypes as c

    x = c.CDLL("libX11.so.6")
    t = c.CDLL("libXtst.so.6")
    x.XOpenDisplay.restype = c.c_void_p
    x.XOpenDisplay.argtypes = [c.c_char_p]
    x.XFlush.argtypes = [c.c_void_p]
    x.XSync.argtypes = [c.c_void_p, c.c_int]
    x.XCloseDisplay.argtypes = [c.c_void_p]
    x.XStringToKeysym.argtypes = [c.c_char_p]
    x.XStringToKeysym.restype = c.c_ulong
    x.XKeysymToKeycode.argtypes = [c.c_void_p, c.c_ulong]
    x.XKeysymToKeycode.restype = c.c_uint
    x.XKeycodeToKeysym.argtypes = [c.c_void_p, c.c_uint, c.c_int]
    x.XKeycodeToKeysym.restype = c.c_ulong
    x.XDisplayWidth.argtypes = [c.c_void_p, c.c_int]
    x.XDisplayHeight.argtypes = [c.c_void_p, c.c_int]
    t.XTestFakeMotionEvent.argtypes = [c.c_void_p, c.c_int, c.c_int, c.c_int, c.c_ulong]
    t.XTestFakeButtonEvent.argtypes = [c.c_void_p, c.c_uint, c.c_int, c.c_ulong]
    t.XTestFakeKeyEvent.argtypes = [c.c_void_p, c.c_uint, c.c_int, c.c_ulong]

    display = x.XOpenDisplay(None)
    require(display, "cannot open the session display")
    try:
        require(
            x.XDisplayWidth(display, 0) == SCREEN_WIDTH and x.XDisplayHeight(display, 0) == SCREEN_HEIGHT,
            "the display is not the controller's 1440x900 private screen",
        )

        def keycode(name):
            sym = x.XStringToKeysym(name.encode())
            require(sym, f"unknown keysym {name}")
            code = x.XKeysymToKeycode(display, sym)
            require(code, f"keysym {name} has no keycode")
            return sym, code

        def press(code, down):
            t.XTestFakeKeyEvent(display, code, 1 if down else 0, 0)
            x.XSync(display, 0)

        shift = keycode("Shift_L")[1]
        control = keycode("Control_L")[1]

        if sub == "key":
            codes = [keycode(name)[1] for name in values[0].split("+")]
            try:
                for code in codes:
                    press(code, True)
                    time.sleep(0.02)
            finally:
                for code in reversed(codes):
                    press(code, False)
        elif sub == "type":
            for ch in values[0]:
                # Printable ASCII keysyms equal their code points.
                code = x.XKeysymToKeycode(display, ord(ch))
                require(code, f"no keycode for {ch!r}")
                shifted = x.XKeycodeToKeysym(display, code, 0) != ord(ch)
                try:
                    if shifted:
                        press(shift, True)
                    press(code, True)
                finally:
                    press(code, False)
                    if shifted:
                        press(shift, False)
                time.sleep(0.01)
        else:
            t.XTestFakeMotionEvent(display, -1, values[0], values[1], 0)
            x.XSync(display, 0)
            if sub == "click":
                t.XTestFakeButtonEvent(display, values[2], 1, 0)
                t.XTestFakeButtonEvent(display, values[2], 0, 80)
            elif sub == "drag":
                t.XTestFakeButtonEvent(display, 1, 1, 0)
                try:
                    x.XSync(display, 0)
                    time.sleep(0.2)
                    for step in range(1, 11):
                        px = values[0] + (values[2] - values[0]) * step // 10
                        py = values[1] + (values[3] - values[1]) * step // 10
                        t.XTestFakeMotionEvent(display, -1, px, py, 30)
                        x.XSync(display, 0)
                finally:
                    t.XTestFakeButtonEvent(display, 1, 0, 80)
            elif sub == "wheel":
                button = 5 if values[2] > 0 else 4
                try:
                    if values[3]:
                        press(control, True)
                    for _ in range(abs(values[2])):
                        t.XTestFakeButtonEvent(display, button, 1, 0)
                        t.XTestFakeButtonEvent(display, button, 0, 20)
                        x.XSync(display, 0)
                finally:
                    if values[3]:
                        press(control, False)
        x.XFlush(display)
        time.sleep(0.2)
    finally:
        x.XCloseDisplay(display)


def main(argv, env):
    check_only = bool(argv) and argv[0] == "--check"
    if check_only:
        argv = argv[1:]
    try:
        validate_environment(env)
        sub, values = validate_arguments(argv)
    except (Refused, OSError) as error:
        print(f"control-native: refusing: {error}", file=sys.stderr)
        return 2
    if check_only:
        print("control-native: ok")
        return 0
    try:
        perform(sub, values)
    except Refused as error:
        print(f"control-native: refusing: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:], os.environ))
