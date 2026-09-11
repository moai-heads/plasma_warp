#!/usr/bin/env python3
"""Overlay the HAND-LABELLED sync points on the track's waveform.

This is a pure verification/viewer tool -- it performs NO onset detection.
It just draws where beats.txt (snare) and kicks.txt (kick) say the hits are,
so you can eyeball them against the audio.

usage: tools/plot_sync.py [ogg] [beats.txt] [kicks.txt] [out.png]
"""
import subprocess, sys
import numpy as np
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

ogg   = sys.argv[1] if len(sys.argv) > 1 else "meltdown_beat.ogg"
bpath = sys.argv[2] if len(sys.argv) > 2 else "beats.txt"
kpath = sys.argv[3] if len(sys.argv) > 3 else "kicks.txt"
out   = sys.argv[4] if len(sys.argv) > 4 else "sync_verification.png"

def read_times(p):
    return np.array([float(l.split("\t")[0]) for l in open(p)
                     if l.strip() and not l.lstrip().startswith("#")])

raw = subprocess.run(["ffmpeg", "-v", "quiet", "-i", ogg, "-f", "f32le",
                      "-ac", "1", "-ar", "48000", "-"],
                     check=True, capture_output=True).stdout
x = np.frombuffer(raw, dtype=np.float32)
sr = 48000
t = np.arange(len(x)) / sr

snare, kick = read_times(bpath), read_times(kpath)

fig, ax = plt.subplots(figsize=(18, 6))
step = 64  # 1.33 ms envelope resolution
n = len(x) // step
env = np.abs(x[:n*step].reshape(n, step)).max(axis=1)
te = np.arange(n) * step / sr
ax.fill_between(te, -env, env, color="0.6", lw=0)
ax.plot(te, env, color="0.35", lw=0.5)

ax.vlines(snare, -1.05, 1.05, color="tab:orange", lw=1.4, label=f"snare ({len(snare)})")
ax.vlines(kick,  -1.05, 1.05, color="tab:blue",   lw=1.4, label=f"kick ({len(kick)})")

ax.set_xlim(0, len(x)/sr)
ax.set_ylim(-1.1, 1.1)
ax.set_xlabel("seconds"); ax.set_ylabel("amplitude")
ax.set_title(f"{ogg} -- hand-labelled sync points")
ax.grid(alpha=0.25); ax.legend(loc="upper right")
fig.tight_layout(); fig.savefig(out, dpi=110)
print(f"wrote {out}  snare={len(snare)} kick={len(kick)}  dur={len(x)/sr:.4f}s")
