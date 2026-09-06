#!/usr/bin/env bash
# Source only from the disposable package tests.
[[ -f /.dockerenv && "$XDG_RUNTIME_DIR" == /tmp/tagrove-* ]]
package_recorder_pid=""
start_package_audio() {
  mkdir -p "$XDG_RUNTIME_DIR/pulse"
  chmod 700 "$XDG_RUNTIME_DIR"
  export PULSE_SERVER="unix:$XDG_RUNTIME_DIR/pulse/native"
  pulseaudio --daemonize=yes --exit-idle-time=-1 -n \
    --load="module-native-protocol-unix socket=$XDG_RUNTIME_DIR/pulse/native auth-anonymous=1" \
    --load="module-null-sink sink_name=tagrove" --log-target=file:/evidence/pulseaudio.log
  parec --device=tagrove.monitor --format=s16le --rate=44100 --channels=1 > /evidence/playback.s16 &
  package_recorder_pid=$!
}
finish_package_audio() {
  kill "$package_recorder_pid"
  wait "$package_recorder_pid" || true
  package_recorder_pid=""
  python3 - <<'PY'
from pathlib import Path
import array, json, math
samples = array.array('h', Path('/evidence/playback.s16').read_bytes())
assert samples, 'No recorded audio'
peak = max(abs(value) for value in samples)
rms = math.sqrt(sum(value * value for value in samples) / len(samples))
assert peak > 200 and rms > 100, (peak, rms)
Path('/evidence/audio-result.json').write_text(json.dumps({
    'samples': len(samples), 'rate': 44100, 'peak': peak, 'rms': rms,
}))
PY
}
