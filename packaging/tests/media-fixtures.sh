#!/usr/bin/env bash
set -euo pipefail
[[ -f /.dockerenv && ! -e /root/tagrove-media ]]
mkdir /root/tagrove-media
# Arguments are the packaged ffmpeg command, optionally prefixed by flatpak run.
"$@" -v error -f lavfi -i color=c=red:s=64x64 -frames:v 1 /root/tagrove-media/red.png
"$@" -v error -f lavfi -i testsrc2=size=320x240:rate=10 -t 2 /root/tagrove-media/sample.gif
# Distinct from the GIF: blue for 2.5 seconds, then yellow. Seeking can be
# verified from pixels without mistaking a previous GIF frame for video.
"$@" -v error -f lavfi -i "color=c=blue:s=320x240:r=25,drawbox=c=yellow:t=fill:enable='gte(t,2.5)'" \
  -f lavfi -i sine=frequency=440 -t 5 -c:v mpeg4 -c:a aac /root/tagrove-media/sample.mp4
# Make the gallery's default newest-first order independent of scan scheduling.
touch -d '2026-01-01 00:00:01 UTC' /root/tagrove-media/red.png
touch -d '2026-01-01 00:00:02 UTC' /root/tagrove-media/sample.gif
touch -d '2026-01-01 00:00:03 UTC' /root/tagrove-media/sample.mp4
