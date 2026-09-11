#!/usr/bin/env bash
# mux_seg.sh -- lock the dev/demo segment muxing recipe, with verification.
#   usage: tools/mux_seg.sh <frames_dir> <audio> <offset_sec> <dur_sec> <out.mp4>
# Audio is LOOPED (so scenes past the end of the track keep playing) and the
# segment starts at <offset_sec> == (demo_start % song_len) computed by the app.
# -ss/-t are INPUT options on the audio; as OUTPUT options they also seek the
# video and silently produce an audio-only file (learned the hard way).
set -euo pipefail
FR=${1:?frames dir}; AUD=${2:?audio}; OFF=${3:?offset}; DUR=${4:?duration}; OUT=${5:?out}
FPS=30
ffmpeg -y -v error -framerate $FPS -i "$FR/f%05d.png" \
       -stream_loop -1 -ss "$OFF" -t "$DUR" -i "$AUD" \
       -map 0:v -map 1:a -c:v libx264 -crf 18 -pix_fmt yuv420p \
       -c:a aac -b:a 192k -shortest "$OUT"

# --- verify: exactly DUR seconds, expected frame count, and REAL audio ---
D=$(ffprobe -v error -show_entries format=duration -of default=nokey=1:noprint_wrappers=1 "$OUT")
N=$(ffprobe -v error -count_frames -select_streams v:0 -show_entries stream=nb_read_frames -of default=nokey=1:noprint_wrappers=1 "$OUT")
WANT=$(python3 -c "print(round($DUR*$FPS))")
VM=$(ffmpeg -i "$OUT" -af volumedetect -f null - 2>&1 | sed -n 's/.*mean_volume: \([^ ]*\).*/\1/p')
echo "muxed $OUT: duration=${D}s frames=${N}/${WANT} mean_volume=${VM} dB"
[ "$N" = "$WANT" ] || { echo "FAIL: frame count"; exit 1; }
python3 -c "import sys; sys.exit(0 if abs(float('$D')-$DUR)<0.05 else 1)" || { echo "FAIL: duration"; exit 1; }
python3 -c "import sys; sys.exit(0 if float('$VM')>-60 else 1)"   || { echo "FAIL: audio silent"; exit 1; }
echo "OK"
