#!/usr/bin/env python3
"""
extract_sync.py -- pull SNARE + KICK onset timestamps out of an audio file.

Pipeline
--------
1. ffmpeg decodes any container (ogg/mp3/wav/flac...) to mono float32 @ 48 kHz.
2. Zero-phase Butterworth band split (sosfiltfilt, so very low bands stay stable):
       HIGH band  > 2 kHz   -> snare  (bright noise crack)
       LOW  band  25-90 Hz  -> kick   (bass thump only)
3. Rectify + moving-average -> energy envelope per band.
4. Onset = rising edge of the envelope (positive first difference) above an
   adaptive threshold (frac * band max), with a refractory gap so one hit
   does not produce several onsets.
5. Timestamps written to text files, one per line, seconds with 4 decimals.

Usage:
    python3 tools/extract_sync.py meltdown_beat.ogg [--outdir .] [--plot check.png]

Outputs (in --outdir):  beats.txt (snare)   kicks.txt (kick)
Optional:               --plot writes a waveform picture with the sync points marked.
"""
import os, sys, argparse, subprocess
import numpy as np
from scipy.signal import butter, sosfiltfilt, find_peaks

SR = 48000

def decode(path):
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", path,
         "-ac", "1", "-ar", str(SR), "-f", "f32le", "-"],
        capture_output=True, check=True).stdout
    return np.frombuffer(raw, dtype="<f4").astype(np.float64)

def band(x, lo=None, hi=None, order=2):
    n = SR / 2
    if lo and hi: sos = butter(order, [lo/n, hi/n], btype="band",   output="sos")
    elif hi:      sos = butter(order, hi/n,        btype="low",    output="sos")
    else:         sos = butter(order, lo/n,        btype="high",   output="sos")
    return sosfiltfilt(sos, x)

def envelope(x, win_ms):
    w = max(1, int(SR * win_ms / 1000))
    return np.convolve(np.abs(x), np.ones(w) / w, mode="same")

def onsets(x, frac, gap_s, win_ms):
    e = envelope(x, win_ms)
    d = np.diff(e, prepend=e[0])
    d[d < 0] = 0.0                                   # attacks only
    pk, _ = find_peaks(d, height=frac * d.max(), distance=int(gap_s * SR))
    out, last = [], -10**9                           # greedy refractory dedup
    for p in pk:
        if p - last >= gap_s * SR:
            out.append(p); last = p
    return np.array(out) / SR

def extract(path):
    x = decode(path)
    snare = onsets(band(x, lo=2000),        frac=0.50, gap_s=0.50, win_ms=8)
    kick  = onsets(band(x, lo=25, hi=90),   frac=0.30, gap_s=0.22, win_ms=12)
    return x, snare, kick

def plot(x, snare, kick, path):
    import matplotlib; matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    t = np.arange(len(x)) / SR
    env = envelope(band(x, lo=40, hi=12000), 6)
    fig, ax = plt.subplots(figsize=(16, 5), dpi=110)
    ax.plot(t, env, lw=0.6, color="0.6", label="envelope")
    for i, s in enumerate(snare):
        ax.axvline(s, color="#d62728", lw=1.0, alpha=.85,
                   label="snare" if i == 0 else None)
    for i, k in enumerate(kick):
        ax.axvline(k, color="#1f77b4", lw=1.0, alpha=.85,
                   label="kick" if i == 0 else None)
    ax.set_xlabel("time (s)"); ax.set_ylabel("envelope")
    ax.set_title("sync points over waveform")
    ax.legend(loc="upper right"); ax.margins(x=0)
    fig.tight_layout(); fig.savefig(path); plt.close(fig)

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("audio")
    ap.add_argument("--outdir", default=".")
    ap.add_argument("--plot", default=None)
    a = ap.parse_args()

    x, snare, kick = extract(a.audio)
    print(f"file     : {a.audio}")
    print(f"duration : {len(x)/SR:.4f}s @ {SR} Hz")
    for name, ts, kind in (("beats.txt", snare, "SNARE"), ("kicks.txt", kick, "KICK")):
        with open(os.path.join(a.outdir, name), "w") as f:
            for t in ts:
                f.write(f"{t:.4f}\n")
        print(f"{kind:>5}   : {len(ts):3d} onsets -> {os.path.join(a.outdir, name)}")
        print("         " + " ".join(f"{t:.3f}" for t in ts))
    if a.plot:
        plot(x, snare, kick, a.plot); print(f"plot     : {a.plot}")

if __name__ == "__main__":
    main()
