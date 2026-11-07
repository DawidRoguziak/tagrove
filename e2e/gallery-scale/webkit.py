"""Run the scale fixture only on the caller's private Xvfb display."""
import ctypes
import json
import os
from pathlib import Path
from functools import partial
from http.server import ThreadingHTTPServer, SimpleHTTPRequestHandler
import subprocess
import sys
import tempfile
import threading

import gi

gi.require_version('Gtk', '3.0')
gi.require_version('WebKit2', '4.1')
gi.require_version('Soup', '3.0')
from gi.repository import Gio, Gtk, WebKit2, GLib, Soup

assert os.environ.get('MEDIATAGGER_SCALE_DISPLAY') == os.environ.get('DISPLAY')
assert os.environ.get('DISPLAY'), 'Run through run.sh on a private display'
output = Path('artifacts/gallery-scale').resolve()
output.mkdir(parents=True, exist_ok=True)
profile = tempfile.TemporaryDirectory(prefix='mediatagger-gallery-scale-')
for key, subdir in [('XDG_DATA_HOME', 'data'), ('XDG_CONFIG_HOME', 'config'), ('XDG_CACHE_HOME', 'cache'), ('XDG_RUNTIME_DIR', 'runtime')]:
    directory = Path(profile.name) / subdir
    directory.mkdir(mode=0o700)
    os.environ[key] = str(directory)

# XTest delivers native input to the isolated server, including WebKit scrollbar dragging.
x11 = ctypes.CDLL('libX11.so.6')
xtst = ctypes.CDLL('libXtst.so.6')
x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XFlush.argtypes = [ctypes.c_void_p]
x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
x11.XStringToKeysym.argtypes = [ctypes.c_char_p]
x11.XStringToKeysym.restype = ctypes.c_ulong
x11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
x11.XKeysymToKeycode.restype = ctypes.c_uint
xtst.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]
xtst.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
xtst.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
display = x11.XOpenDisplay(None)
assert display

class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass

server = ThreadingHTTPServer(('127.0.0.1', 0), partial(QuietHandler, directory=str(output / 'dist')))
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
context = WebKit2.WebContext.new_ephemeral()

def serve_thumbnail(request):
    """Synthetic `thumb://localhost/<id>?v=...`: an SVG after 20 ms, like a cached file."""
    asset_id = int(request.get_path().strip('/') or 0)
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" width="188" height="188">'
           f'<rect width="188" height="188" fill="hsl({asset_id % 360},60%,35%)"/>'
           f'<circle cx="94" cy="78" r="48" fill="hsl({(asset_id + 80) % 360},60%,65%)"/>'
           f'<text x="12" y="172" fill="white" font-size="22">{asset_id}</text></svg>').encode()
    def respond():
        stream = Gio.MemoryInputStream.new_from_bytes(GLib.Bytes.new(svg))
        response = WebKit2.URISchemeResponse.new(stream, len(svg))
        response.set_content_type('image/svg+xml')
        headers = Soup.MessageHeaders.new(Soup.MessageHeadersType.RESPONSE)
        headers.append('Access-Control-Allow-Origin', '*')
        response.set_http_headers(headers)
        request.finish_with_response(response)
        return False
    GLib.timeout_add(20, respond)

context.register_uri_scheme('thumb', serve_thumbnail)
context.get_security_manager().register_uri_scheme_as_secure('thumb')
view = WebKit2.WebView.new_with_context(context)
window = Gtk.Window()
window.set_decorated(False)
window.move(0, 0)
window.set_default_size(1800, 850)
window.add(view)
state = {'busy': False, 'action': 0, 'exit': 1}
manager = view.get_user_content_manager()
manager.register_script_message_handler('errors')
manager.connect('script-message-received::errors', lambda _, result: print(result.get_js_value().to_string(), flush=True))
manager.add_script(WebKit2.UserScript.new('''
window.addEventListener('error', e => window.webkit.messageHandlers.errors.postMessage(e.message));
window.addEventListener('unhandledrejection', e => window.webkit.messageHandlers.errors.postMessage(String(e.reason)));
''', WebKit2.UserContentInjectedFrames.ALL_FRAMES, WebKit2.UserScriptInjectionTime.START, None, None))

def execute_steps(steps, action_id):
    if not steps:
        view.evaluate_javascript(f'window.nativeDone = {action_id}', -1, None, None, None, None, None)
        return False
    step, *remaining = steps
    kind = step['type']
    delay = step.get('ms', 60)
    if kind == 'move':
        xtst.XTestFakeMotionEvent(display, -1, round(step['x']), round(step['y']), 0)
    elif kind == 'button':
        xtst.XTestFakeButtonEvent(display, step['button'], step['down'], 0)
    elif kind == 'key':
        code = x11.XKeysymToKeycode(display, x11.XStringToKeysym(step['key'].encode()))
        xtst.XTestFakeKeyEvent(display, code, step['down'], 0)
    elif kind == 'pause':
        delay = step['ms']
    elif kind == 'screenshot':
        assert step['name'] in ['last-result', 'rectangle-selection', 'performance-thumbnails', 'moderate-scroll',
                                'million-oldest-month', 'million-select-all'] or (
            step['name'].startswith('height-probe-') and step['name'].replace('-', '').isalnum())
        subprocess.run(['import', '-window', 'root', str(output / f"{step['name']}.png")], check=True)
    else:
        raise ValueError(kind)
    x11.XFlush(display)
    GLib.timeout_add(delay, execute_steps, remaining, action_id)
    return False

def complete(obj, result, *_):
    state['busy'] = False
    try:
        data = json.loads(obj.evaluate_javascript_finish(result).to_string())
        action = data.get('action')
        if action and action['id'] > state['action']:
            state['action'] = action['id']
            execute_steps(action['steps'], action['id'])
        result = data.get('result')
        if result:
            result['webkit'] = '.'.join(str(f()) for f in [WebKit2.get_major_version, WebKit2.get_minor_version, WebKit2.get_micro_version])
            (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
            print(json.dumps(result), flush=True)
            state['exit'] = 0 if result['ok'] else 1
            Gtk.main_quit()
    except Exception as error:
        print(repr(error), flush=True)
        Gtk.main_quit()

def poll():
    if not state['busy'] and not view.is_loading():
        state['busy'] = True
        view.evaluate_javascript('JSON.stringify({result: window.scaleResult, action: window.nativeAction})', -1, None, None, None, complete, None)
    return True

def timeout():
    print('TIMEOUT', flush=True)
    Gtk.main_quit()
    return False

window.show_all()
page = ('performance.html' if os.environ.get('GALLERY_PERFORMANCE') == '1'
        else 'million.html' if os.environ.get('GALLERY_MILLION') == '1'
        else 'height-probe.html' if os.environ.get('GALLERY_HEIGHT_PROBE') == '1' else 'index.html')
view.load_uri(f'http://127.0.0.1:{server.server_port}/{page}')
GLib.timeout_add(50, poll)
GLib.timeout_add_seconds(55, timeout)
try:
    Gtk.main()
finally:
    subprocess.run(['import', '-window', 'root', str(output / 'final.png')], check=False)
    window.destroy()
    server.shutdown()
    server.server_close()
    thread.join(timeout=5)
    x11.XCloseDisplay(display)
    profile_path = profile.name
    profile.cleanup()
    (output / 'cleanup.json').write_text(json.dumps({'profile_removed': not Path(profile_path).exists(), 'server_thread_stopped': not thread.is_alive()}))
sys.exit(state['exit'])
