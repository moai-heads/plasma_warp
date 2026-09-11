# plasma_warp

A scene-based, **music-synced** realtime demo written in Rust. One ~1400-line
`main.rs`, no engine, no ECS — just a software rasterizer and three scenes
stitched on a timeline that is locked to hand-labelled beat onsets.

Timeline (demo-local seconds, loops):

| scene | starts | len | what |
|---|---|---|---|
| **Rotozoom** | 0.0 s | 16 s | plasma warp + ripple + rotozoom |
| **TriangleDance** | 16.0 s | 16 s | 3D pyramid over a blurred background |
| **CyberPuzzle** | 32.0 s | 8.2 s | image shatters into tumbling 3D pieces, streams back in from depth, unifies **on a snare**, sways on the hihat grid, then flips 180° to reveal a second texture |

## Requirements

- **Rust** 1.80+ (verified on 1.98.1) and Cargo.
- **SDL3** development package, for the default (realtime) build:
  - Arch / Manjaro: `sudo pacman -S sdl3`
  - Debian / Ubuntu: `sudo apt install libsdl3-dev`
  - macOS: `brew install sdl3`
  - Windows: SDL3 via `vcpkg` (`cargo` will try pkg-config then vcpkg)
- **No SDL needed** for the headless frame-dumper build.

There are no other C dependencies: OGG/Vorbis decoding is pure Rust (`lewton`),
image decoding is `image`.

## Build & run — realtime window + music (default)

```sh
cargo run --release
```

Opens a resizable SDL3 window (renders at 640×360, scaled + letterboxed) and
plays `meltdown_beat.ogg` underneath, in sync with the visual timeline.

- **Esc** or closing the window quits.
- The demo is a software renderer at 30 fps; on a slow machine a release build
  is strongly recommended (a debug build renders the heavy scenes at ~½ speed).

## Build & run — headless frame dumper

Writes numbered PNG frames (for ffmpeg), no SDL, no audio:

```sh
# whole timeline -> frames/f00000.png ...
cargo run --release --no-default-features -- demo

# one scene only (`dev` also prints the audio offset to mux with)
cargo run --release --no-default-features -- dev cyberpuzzle
cargo run --release --no-default-features -- dev rotozoom
cargo run --release --no-default-features -- dev triangledance
```

With the default `realtime` feature you can still ask for the dumper by name:
`cargo run --release -- demo`.

## Assets

Loaded from the **current working directory** (run `cargo run` from the project
root). Override the directory with `PLASMA_ASSET_DIR=/path/to/assets`, and the
frame output dir with `PLASMA_FRAMES_DIR`.

```
tex_purple.png  tex_green.png  tex_blue.png       # Rotozoom + TriangleDance
tex_scene3.png  tex_scene4.png                    # CyberPuzzle front / back
meltdown_beat.ogg                                 # the track
beats.txt  kicks.txt                              # hand-labelled onsets (seconds)
```

`beats.txt` (snare) and `kicks.txt` (kick) are **hand-labelled in Audacity** and
are the single source of truth for sync — no onset detection exists anywhere in
this project. If timing is wrong, fix the labels, not the code.

## Cargo features

| feature | default | effect |
|---|---|---|
| `realtime` | **yes** | SDL3 window + music playback (`sdl3`, `lewton`) |
| *(none)* | | headless PNG frame dumper (image only) |

## Handy environment knobs

| var | effect |
|---|---|
| `PLASMA_ASSET_DIR` | asset directory (default `.`) |
| `PLASMA_FRAMES_DIR` | frame output dir (default `frames`) |
| `PLASMA_MAX_FRAMES` | realtime: exit after N frames (for smoke tests) |
| `PLASMA_ROTOZOOM_SYNC` | `snare` (default) or `kick` — rotozoom punch source |
| `PLASMA_NO_SNARE` / `PLASMA_NO_KICK` | silence a channel (for A/B diffs) |
| `CYBERPUZZLE_*` | per-effect tuning: `COLS`, `ROWS`, `SHAPE_AMP`, `ORIGIN_Z`, `EASE_K`, `DANCE_AMP`, `FORCE_S`, `TINT`, `SPIN`, `TIMING` |

## Layout

```
src/main.rs      everything: renderer, scenes, sync, timeline, SDL3 player
tex_*.png        textures
meltdown_beat.ogg + beats.txt + kicks.txt
tools/mux_seg.sh ffmpeg mux + verify recipe for the headless output
tools/plot_sync.py overlay the labels on the waveform (verification)
```
