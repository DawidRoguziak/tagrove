"""Drive the release package on its disposable 1440x900 X11 display."""
from pathlib import Path
import json
import sqlite3
import struct
import subprocess
import sys
import time

assert Path('/.dockerenv').exists()
database = Path(sys.argv[1])
assert str(database).startswith(('/tmp/tagrove-data/', '/root/.var/app/com.example.mediatagger/'))
helper = Path(__file__).with_name('x11-input.py')


def input(*arguments):
    subprocess.run([sys.executable, str(helper), *map(str, arguments)], check=True)
    time.sleep(0.35)


def screenshot(name):
    target = Path('/evidence') / (name + '.xwd')
    subprocess.run([sys.executable, str(helper), 'screenshot', str(target)], check=True)
    return target.read_bytes()


def rgb(data, x, y):
    header = struct.unpack('>25I', data[:100])
    offset = header[0] + header[19] * 12 + y * header[12] + x * (header[11] // 8)
    pixel = int.from_bytes(data[offset:offset + header[11] // 8], 'little' if header[7] == 0 else 'big')
    values = []
    for mask in header[14:17]:
        shift = (mask & -mask).bit_length() - 1
        values.append((pixel & mask) >> shift)
    return values


def colorful(data):
    return sum(max(rgb(data, x, y)) - min(rgb(data, x, y)) > 100
               for x in range(150, 1000, 100) for y in range(100, 800, 100)) > 30


def video_color(data, color):
    pixels = [rgb(data, x, y) for x in range(200, 1000, 73) for y in range(120, 730, 61)]
    if color == 'blue':
        return sum(r < 40 and g < 40 and b > 200 for r, g, b in pixels) > len(pixels) * 0.9
    return sum(r > 200 and g > 200 and b < 40 for r, g, b in pixels) > len(pixels) * 0.9


def await_pixels(name, predicate):
    for attempt in range(30):
        time.sleep(0.5)
        if predicate(screenshot(name)):
            return
    raise AssertionError(f'{name} did not render expected fixture pixels')


input('focus-name', 'Tagrove')
input('click', 720, 568)  # Add first folder -> settings.
input('click', 1256, 126)
time.sleep(2)
input('focus-name', 'Select media folder')
input('click', 58, 59)  # Container home contains only the fixture directory.
time.sleep(1)
screenshot('folder-dialog')
input('click', 241, 84)
input('click', 1046, 798)
time.sleep(1)
input('focus-name', 'Tagrove')
input('click', 1184, 257)  # Rescan the one root.
for attempt in range(60):
    with sqlite3.connect(f'file:{database}?mode=ro', uri=True) as connection:
        rows = connection.execute('select path, thumb_path from assets').fetchall()
    if len(rows) == 3:
        break
    time.sleep(0.5)
else:
    raise AssertionError(f'Scan did not complete: {rows}')
assert all(row[0].startswith('/root/tagrove-media/') for row in rows)
input('click', 74, 34)
for attempt in range(60):
    with sqlite3.connect(f'file:{database}?mode=ro', uri=True) as connection:
        rows = connection.execute('select path, thumb_path from assets').fetchall()
    if all(row[1] for row in rows):
        break
    time.sleep(0.5)
else:
    raise AssertionError(f'Thumbnail generation did not complete: {rows}')
screenshot('gallery')
input('click', 506, 211)
time.sleep(1)
red = rgb(screenshot('image'), 500, 400)
assert red[0] > 200 and red[1] < 40 and red[2] < 40, red
input('click', 1212, 362)
input('type', 'packageverified')
input('key', 'Return')
time.sleep(1)
screenshot('tagged')
with sqlite3.connect(f'file:{database}?mode=ro', uri=True) as connection:
    tags = connection.execute('select a.file_name,t.name from assets a join asset_tags at on at.asset_id=a.id join tags t on t.id=at.tag_id').fetchall()
assert ('red.png', 'packageverified') in tags, tags
input('click', 1391, 49)
input('click', 315, 212)
await_pixels('gif', colorful)
input('click', 1391, 49)
input('click', 110, 212)
await_pixels('video', lambda data: video_color(data, 'blue') or video_color(data, 'yellow'))
input('click', 41, 834)
input('click', 435, 834)
await_pixels('video-seek-start', lambda data: video_color(data, 'blue'))
input('click', 845, 834)
await_pixels('video-seek-end', lambda data: video_color(data, 'yellow'))
input('click', 1081, 834)
await_pixels('video-fullscreen', lambda data: video_color(data, 'yellow') and
             rgb(data, 1200, 400)[0] > 200 and rgb(data, 1200, 400)[1] > 200)
Path('/evidence/media-ui.json').write_text(json.dumps({'assets': rows, 'tags': tags, 'rendered': ['image', 'gif', 'video', 'video-fullscreen']}, indent=2))
print('PASS: native folder chooser, scan, thumbnails, persisted tag, image/GIF/video pixels, seek and fullscreen controls')
