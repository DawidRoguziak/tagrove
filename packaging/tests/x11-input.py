"""Send input only to the private display in a disposable package test container."""
import ctypes as C
from pathlib import Path
import sys
import struct


class XImage(C.Structure):
    _fields_ = [(name, C.c_int) for name in ('width', 'height', 'xoffset', 'format')] + [
        ('data', C.c_void_p),
    ] + [(name, C.c_int) for name in ('byte_order', 'bitmap_unit', 'bitmap_bit_order', 'bitmap_pad', 'depth', 'bytes_per_line', 'bits_per_pixel')] + [
        ('red_mask', C.c_ulong), ('green_mask', C.c_ulong), ('blue_mask', C.c_ulong),
    ]

if not Path('/.dockerenv').exists():
    raise SystemExit('This helper is restricted to the disposable package test container')
x11 = C.CDLL('libX11.so.6')
xtst = C.CDLL('libXtst.so.6')
x11.XOpenDisplay.argtypes = [C.c_char_p]
x11.XOpenDisplay.restype = C.c_void_p
x11.XStringToKeysym.argtypes = [C.c_char_p]
x11.XStringToKeysym.restype = C.c_ulong
x11.XKeysymToKeycode.argtypes = [C.c_void_p, C.c_ulong]
x11.XKeysymToKeycode.restype = C.c_uint
x11.XSync.argtypes = [C.c_void_p, C.c_int]
x11.XSetInputFocus.argtypes = [C.c_void_p, C.c_ulong, C.c_int, C.c_ulong]
x11.XDefaultRootWindow.argtypes = [C.c_void_p]
x11.XDefaultRootWindow.restype = C.c_ulong
x11.XQueryTree.argtypes = [C.c_void_p, C.c_ulong, C.POINTER(C.c_ulong), C.POINTER(C.c_ulong), C.POINTER(C.POINTER(C.c_ulong)), C.POINTER(C.c_uint)]
x11.XFetchName.argtypes = [C.c_void_p, C.c_ulong, C.POINTER(C.c_char_p)]
x11.XFree.argtypes = [C.c_void_p]
x11.XGetImage.argtypes = [C.c_void_p, C.c_ulong, C.c_int, C.c_int, C.c_uint, C.c_uint, C.c_ulong, C.c_int]
x11.XGetImage.restype = C.POINTER(XImage)
x11.XDestroyImage.argtypes = [C.POINTER(XImage)]
x11.XDisplayWidth.argtypes = [C.c_void_p, C.c_int]
x11.XDisplayHeight.argtypes = [C.c_void_p, C.c_int]
xtst.XTestFakeMotionEvent.argtypes = [C.c_void_p, C.c_int, C.c_int, C.c_int, C.c_ulong]
xtst.XTestFakeButtonEvent.argtypes = [C.c_void_p, C.c_uint, C.c_int, C.c_ulong]
xtst.XTestFakeKeyEvent.argtypes = [C.c_void_p, C.c_uint, C.c_int, C.c_ulong]
display = x11.XOpenDisplay(b':97')
if not display:
    raise SystemExit('Private package test display is not running')


def keycode(key):
    symbol = ord(key) if len(key) == 1 else x11.XStringToKeysym(key.encode())
    code = x11.XKeysymToKeycode(display, symbol)
    if not code:
        raise ValueError(f'No keycode for {key}')
    return code


def key(key, pressed):
    xtst.XTestFakeKeyEvent(display, keycode(key), int(pressed), 0)


command = sys.argv[1]
if command == 'screenshot':
    # Request synchronized server pixels, including GL rendering.
    width, height = x11.XDisplayWidth(display, 0), x11.XDisplayHeight(display, 0)
    captured = x11.XGetImage(display, x11.XDefaultRootWindow(display), 0, 0, width, height, C.c_ulong(-1), 2)
    if not captured:
        raise SystemExit('Could not capture private display')
    frame = captured.contents
    name = b'Tagrove package test\0'
    header = struct.pack('>25I', 100 + len(name), 7, 2, frame.depth, width, height,
                         frame.xoffset, frame.byte_order, frame.bitmap_unit, frame.bitmap_bit_order,
                         frame.bitmap_pad, frame.bits_per_pixel, frame.bytes_per_line, 4,
                         frame.red_mask, frame.green_mask, frame.blue_mask, 8, 256, 0,
                         width, height, 0, 0, 0)
    pixels = C.string_at(frame.data, frame.bytes_per_line * frame.height)
    x11.XDestroyImage(captured)
    target = Path(sys.argv[2])
    target.relative_to('/evidence')
    target.write_bytes(header + name + pixels)
elif command == 'focus-name':
    root, parent, count = C.c_ulong(), C.c_ulong(), C.c_uint()
    children = C.POINTER(C.c_ulong)()
    x11.XQueryTree(display, x11.XDefaultRootWindow(display), C.byref(root), C.byref(parent), C.byref(children), C.byref(count))
    found = False
    for index in reversed(range(count.value)):
        name = C.c_char_p()
        if x11.XFetchName(display, children[index], C.byref(name)) and name.value:
            matches = name.value.decode() == sys.argv[2]
            x11.XFree(name)
            if matches:
                x11.XSetInputFocus(display, children[index], 1, 0)
                found = True
                break
    x11.XFree(children)
    if not found:
        raise SystemExit(f'No window named {sys.argv[2]}')
elif command == 'focus':
    x11.XSetInputFocus(display, int(sys.argv[2], 0), 1, 0)
elif command == 'click':
    xtst.XTestFakeMotionEvent(display, 0, int(sys.argv[2]), int(sys.argv[3]), 0)
    xtst.XTestFakeButtonEvent(display, 1, 1, 0)
    xtst.XTestFakeButtonEvent(display, 1, 0, 0)
elif command == 'type':
    for char in sys.argv[2]:
        shift = char.isupper() or char in '~!@#$%^&*()_+{}|:"<>?'
        if shift:
            key('Shift_L', True)
        key(char, True)
        key(char, False)
        if shift:
            key('Shift_L', False)
elif command == 'key':
    for name in sys.argv[2:]:
        key(name, True)
    for name in reversed(sys.argv[2:]):
        key(name, False)
else:
    raise SystemExit('Expected focus WINDOW, click X Y, type TEXT, or key KEY...')
x11.XSync(display, 0)
