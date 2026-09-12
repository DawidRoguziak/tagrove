"""Check only decoded picture pixels, excluding GTK controls and the sidebar."""
import json
import sys
from PIL import Image, ImageChops, ImageStat

first, second = (Image.open(file).convert("RGB") for file in sys.argv[1:3])
x, y, width, height = map(float, sys.argv[3:])
box = tuple(map(round, (x + width * .1, y + height * .1,
                        x + width * .7, y + height * .7)))
a, b = first.crop(box), second.crop(box)
color_fraction = sum(max(p) - min(p) > 60 for p in b.getdata()) / (b.width * b.height)
mean_difference = sum(ImageStat.Stat(ImageChops.difference(a, b)).mean) / 3
assert color_fraction > .2, f"Decoded test pattern missing: {color_fraction}"
assert mean_difference > 1, f"Video picture did not advance: {mean_difference}"
print(json.dumps({"colorFraction": color_fraction, "meanPixelDifference": mean_difference}))
