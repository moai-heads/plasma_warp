# AGENTS.md — plasma_warp (project-scoped)

Rules for this project only. They apply to anyone working in plasma_warp,
human or agent. A project-local AGENTS.md is independent of any workspace-root
file; this one is ours, versioned in the repo, and safe to read and edit here.

## 1. Chat etiquette — DO NOT PASTE CODE
- **NEVER paste source code into Discord messages.** Not full files, not large
  snippets, no "here's the full source" dumps. It turns the thread into a slop
  salad.
- Source code ships as a **file attachment** (send `main.rs` / the script).
  A couple of lines inline to make a point is fine; a whole file never is.
- Same for other bulky text (sync data, logs, build output): attach a file,
  don't inline it.

## 1b. Message formatting — code blocks NEVER span messages
- **Every fenced code block must open AND close inside ONE Discord message.** A
  fence that straddles a message boundary renders as broken formatting: the
  opening ```` ``` ```` lands in message 1, the closing ```` ``` ```` in message
  2, and the client shows raw backticks + swallows the block.
- Assume NOTHING about where the client splits a reply. The agent does not
  control message boundaries (see below), so treat every block as if it could be
  cut at any point.
- Long output → **multiple short, independently-closed blocks**, not one giant
  block. Rule of thumb: keep any single fenced block comfortably short (well
  under ~1500 chars) and, when a listing/table/log is longer, break it into
  several complete blocks with normal prose between them.
- Never leave a dangling opening fence at the end of an output chunk. If a block
  might not fit, close it early and continue in a fresh block.

### Can the agent read/edit its own sent messages?
No. The tooling available here is the sandbox shell, file-send, and web search —
there is no Discord read/edit/delete capability. The agent therefore **cannot
see the individual messages it sent**, cannot confirm where a split occurred,
and cannot fix or retract a broken message after the fact. The only lever is to
format conservatively up front: keep fences short and self-contained.

## 2. Output hygiene
- Renders (frames/, *.mp4, *.gif, intermediates) are DELIVERABLES ONLY.
- After sending, DELETE them from the project. Keep only source, config, and
  small persistent assets (textures, sync labels). Renders rebuild on demand.

## 3. Build
- Two build paths, both from the SAME source (`src/main.rs`):
  - **Packaged / user-facing — Cargo.** `cargo run --release` = realtime SDL3
    window + music (the `realtime` feature, on by default). Headless dumper:
    `cargo run --release --no-default-features -- demo` (or `-- dev <scene>`).
    This is the build the user runs on their own machine.
  - **Sandbox dev — direct `rustc`** against the shared rlib vault at
    /root/rustlib/ (see /root/POLICY/rust.md). Use this for headless iteration
    here:
    `rustc --edition 2021 -O src/main.rs --extern image=/root/rustlib/libimage.rlib --extern bumpalo=/root/rustlib/libbumpalo.rlib --extern arrayvec=/root/rustlib/libarrayvec.rlib -L dependency=/root/rustlib -o app`
- Always recompile before rendering; never claim a fix from a stale binary.

## 4. Sync data — hand-labelled ground truth
- `beats.txt` = snare onsets, `kicks.txt` = kick onsets, both HAND-LABELLED in
  Audacity and committed.
- No onset-detection code anywhere in this project. If sync is wrong, fix the
  labels, not the code. Never regenerate these files with a detector.

## 5. Verification before send
- Every deliverable is verified BEFORE it is posted: frame count == dur*FPS,
  duration correct, audio not silent, and the change actually present in the
  pixels (diff/inspect, don't eyeball).
- No "it's fixed" claims on an unverified build.

## 6. Git
- Every change is a commit with a message saying what changed and why.
  Tag milestones. History is the version-jump mechanism — no hallucinated
  "previous versions".

## 7. Cargo & disk hygiene (packaging policy)
- Cargo is permitted for exactly two things: (a) packaging a buildable project
  for the user, and (b) the throwaway-rlib protocol in /root/POLICY/rust.md.
  Sandbox *development* stays on direct `rustc`.
- Always build with `CARGO_TARGET_DIR=/tmp/...` so `target/` never lands in the
  repo (`.gitignore` also carries `/target`).
- `cargo check` / `cargo build` is allowed to prove the crate compiles — this
  downloads `sdl3`, `lewton`, `image` (only the `png` feature) and their deps from crates.io. Fine.
- **Once the check passes and the project is packaged, purge every byte cargo
  downloaded, immediately:** remove `~/.cargo/registry`, `~/.cargo/.global-cache`
  and the `CARGO_TARGET_DIR`. No cargo build residue survives on the disk.
- `Cargo.lock` IS committed (reproducible builds); `target/` and the registry
  are NOT.
- Deliverable = one zip: source + `Cargo.toml` + `Cargo.lock` + `README.md` +
  assets. Never ship `target/` or a populated cargo cache.

## 7b. Asset secrecy (media must NEVER reach GitHub)
- **Media assets = image + sound**, without exception: `*.png *.jpg *.jpeg
  *.gif *.bmp *.webp *.tga *.tif *.ogg *.mp3 *.wav *.m4a *.aac *.flac *.opus
  *.mp4 *.mov *.avi *.mkv`. They are git-ignored and distributed separately.
- **Sync data (`beats.txt`, `kicks.txt`) IS tracked** and may live on GitHub.
- Before ANY push, verify the *entire* object store, not just the tip tree:
  `git cat-file --batch-all-objects --batch-check=...` + magic-byte scan
  (`89 50 4e 47` PNG, `4f 67 67 53` Ogg, `00 00 00 1c` M4A, `ff d8 ff` JPEG).
- If an asset ever reached a **public** remote, `git push --force` is NOT
  enough: it moves the branch pointer, but the old objects stay fetchable by
  SHA until GitHub GCs (which it does not do on request). **The only reliable
  remedy is: delete the remote repo, purge the asset from local history
  (`git filter-branch`/`filter-repo`), then recreate + push.** (Done once,
  2026-09-11, when `seg.m4a` leaked via history.)

## 8. Audio (SDL3_mixer)
- Realtime playback uses **SDL3_mixer** (`sdl3` crate `mixer` feature) — it
  loads `meltdown_beat.ogg` and loops it. No hand-rolled PCM/lewton path.
- Requires the `sdl3_mixer` system dev package (plus SDL3). The headless dumper
  build (`--no-default-features`) needs neither.
- **LOOPING GOTCHA (fixed 2026-09-12):** `MIX_SetTrackLoops` (Rust `track.set_loops()`)
  called BEFORE `play()` is a **no-op** — per the SDL3_mixer docs, starting a stopped
  track REPLACES the loop count, so the pre-play value is discarded and the track plays
  exactly once. Set looping AT START via `play_with_options` with the property
  `"SDL_mixer.play.loops" = -1` (string value of `MIX_PROP_PLAY_LOOPS_NUMBER`), or call
  `set_loops(-1)` AFTER `play()`. The `sdl3` crate's own mixer example has this bug; it
  hides because the example only plays ~11s of a ~28s file. Symptom if wrong: music plays
  one pass then silence. Verify by capturing with `SDL_AUDIODRIVER=disk` for > song length
  and checking RMS does not drop to zero.

## 9. Git workflow — ALWAYS push
- **Every new commit MUST be pushed to `origin` (`https://github.com/moai-heads/plasma_warp.git`,
  branch `master`) in the same session it is made.** Do not leave commits local-only.
- Verify after pushing: `git status -sb` shows no `ahead` and `git ls-remote origin master`
  matches local `HEAD`.
- If `git push` fails auth, run `gh auth setup-git` first (git's credential helper must be
  wired to the `gh` CLI), then retry.
- Media assets are NEVER pushed (see §7b) — verify the object store before every push.

## 10. Naming — no abbreviations, be explicit
- **Function names must NOT use abbreviations.** Spell the concept out in
  full: a reader should not have to decode `bary`, `tri`, `proj`, `calc`,
  `tmp`, `cfg`, `ctx`, `buf`, `len`-style stems.
  - Write `triangle_barycentric_weights`, not `tri_bary`.
  - Write `triangle_signed_area`, not `tri_area`.
  - Write `triangle_bounding_box`, not `tri_bbox`.
  - Write `project_point`, not `proj`. Write `rotate_xyz`, not `rot3`.
- Names must be **clear and self-describing**: the name alone should say what
  the function computes/does and, where non-obvious, its units or convention
  (e.g. `triangle_signed_area` - "signed" tells you the winding sign matters).
- Applies to new code AND to renames: when touching a function whose name is
  abbreviated, rename it as part of the change.
- Local variables and short-lived loop indices are exempt (a loop counter `i`,
  `w0/w1/w2` barycentric weights, `x/y` pixel coords are fine); this rule is
  about the *names of functions* a stranger has to navigate by.

## 11. Memory / allocation policy — bumpalo per frame, Vec for persistent
- **Persistent data** that lives for the whole program run (textures, `FrameBuffers`,
  timeline, sync labels, anything allocated once at startup) uses ordinary std
  `Vec`/`Box`. It is allocated once and never reallocated inside the loop.
- **Anything allocated from scratch every frame** (scratch lists, temporary
  geometry, per-frame containers) MUST use **bumpalo**. There is exactly ONE
  `Bump` per program run, created OUTSIDE the main render loop and passed down by
  reference: `&Bump` wherever something is allocated; `&mut Bump` only at the loop
  level, solely to call `reset()`.
- **`reset()` the bump once per frame**, after all per-frame bump data is done, so
  the arena is reused rather than grown without bound.
- bumpalo does not free individual allocations and does NOT run destructors on
  `reset()`. Only put plain/POD per-frame data in it (`f32`, `V3`, `[f32; 5]`,
  tuples). NEVER put anything that owns a heap resource into the bump.
- For container types bumpalo has no arena version of (e.g. `HashMap`), keep a
  **scratch pool**: a struct owning the container, alive outside the loop (or
  inside `FrameBuffers`), passed by `&mut`, cleared in place (`clear()`) and
  reused by each user in turn. Treat it as borrowable scratch, not storage.
- **Fixed-capacity-by-construction** containers use **arrayvec**
  (`ArrayVec<T, N>`, zero heap): e.g. the near-clip polygon and the quad corners.
- Vault rlibs: `libbumpalo.rlib` (built with feature `collections`),
  `libarrayvec.rlib`, `libsmallvec.rlib` (see /root/rustlib/MANIFEST). The Cargo
  deps mirror these (bumpalo with `collections`, arrayvec).

## 12. Depth-test convention — depth MUST be view-space distance
- Both rasterizers test `z < depth[idx]` with the buffer initialized to
  `f32::INFINITY`, so **smaller z = nearer**. Therefore the `dz`/`invz` passed
  in must be a *view-space distance to the camera*, never a raw object/model
  coordinate that only happens to be z-ish.
- `raster_tex_tri` (CyberPuzzle) already obeys this: it stores real view depth
  `z = 1/(1/z)`.
- `draw_pyramid` (MenInBlack) projects with `scale/(persp - p.z)`, i.e. the
  camera sits at `z = +persp` looking down `-z`, so view distance is
  `persp - z_rotated`. It passes `persp - rv[k].2` to `fill_tri_flat`. Passing
  the raw `rv[k].2` (done until 2026-09-12) INVERTED the test — the buffer kept
  the farthest face, so back faces drew over front faces, reading as "double
  layered" triangles. If you add a mesh, convert your vertex depth to view
  distance before handing it over.

## 13. Transparent mesh = additive; no refract/glass shader
- The MenInBlack mesh is flat-shaded. When it turns transparent (mesh_alpha<1,
  and for the explosion shards) it blends ADDITIVELY (`Blend::Add`, `o+s*alpha`).
- The old `Surface`/`SurfaceKind`/`MeshPunch::Glass`/`Blend::Screen` machinery and
  the `sample_bg` refraction shader were removed 2026-09-12 (broken slop: it read
  the framebuffer it was writing, so output depended on draw order).
- `fill_tri_flat` now branches on `Blend`: `Alpha` fragments are depth-tested and
  write depth (opaque occlusion, correct); `Add` fragments are NOT depth-gated
  (additive compositing is order-independent — every face must blend, otherwise
  the nearest face would cull the rest).
- Consequence: stacked additive faces clamp at 255 (more glass = brighter). That
  is the intended look now.
- Color split: the OPAQUE mesh uses the blue `base_col` palette; the transparent
  (additive) mesh uses a single bright green `glass_col`, so transparency reads
  as green glow. Flat lambert (`lam`) still varies per face, keeping the 3D form.

## 14. Texture alpha cutout (raster_tex_tri)
- `raster_tex_tri` samples texel ALPHA and DISCARDS texels with `a < TEX_ALPHA_CUTOFF`
  (0.5): no color, no depth write, so whatever is already behind shows through.
- `Tex::sample_clamp_rgba` supplies (rgb, alpha); textures with no alpha channel
  sample alpha 1.0, so the test is a NO-OP for opaque textures (the CyberPuzzle
  FRONT texture is byte-identical before/after). Only textures with real cutouts
  are affected — currently the CyberPuzzle BACK texture (tex_scene4, ~40%
  transparent), so the 180-deg flip now reveals the background through the holes
  instead of drawing the lavender fill under the transparent region.

## 15. CyberPuzzle background — laser beams
- The CyberPuzzle background is drawn FIRST, right after `clear_image`/
  `depth.fill`, before any puzzle piece: `draw_laser_beams(img, st)`.
- Effect: horizontal RED beams spanning the FULL screen width. Each beam is
  hashed off its spawn index, so its vertical position (+ thickness + gain) is
  randomized but DETERMINISTIC — renders are reproducible, no wall-clock RNG.
- Life cycle, total `LASER_BEAM_LIFETIME = 0.40 s`: expand vertically over
  `LASER_BEAM_EXPAND_SECS = 0.15`, hold to `LASER_BEAM_HOLD_SECS = 0.22`, then
  fade out to 0. Spawn every `LASER_BEAM_SPAWN_INTERVAL = 0.12 s`, which is
  `< lifetime/3` so AT LEAST 3 beams are alive at any instant.
- Vertical profile is a gradient: almost-white red core -> pure red -> fully
  transparent at the expanding edge. Blending is ADDITIVE over the black
  background, which is what makes the outer edge disappear cleanly.
- SCENE LENGTH: the CyberPuzzle timeline entry was extended 8.2 s -> 14.0 s on
  2026-09-13. Reason: the 180-deg flip completes at st≈8.093 s, i.e. the old
  8.2 s entry ended only ~3 frames after the flip, so the post-flip background
  had nowhere to play. 14.0 s gives ~5.9 s of beam background after the reveal.
- The beams BEGIN only after the 180-degree flip is COMPLETE: the scene computes
  `beam_start = flip_start + CYBERPUZZLE_FLIP_DUR` and passes the elapsed time
  (`st - beam_start`) to `draw_laser_beams`, which returns early (no spawn, k<0
  guard) while that is negative. So no beam is drawn during the fly-in, assembly,
  jiggle, or the flip itself.
- The letterbox margins (the black bars left/right of the mosaic) are OCCLUDED
  with opaque black by `fill_surround_black`, called AFTER the beams and BEFORE
  the pieces, so no beam is ever visible "past" the puzzle: the camera sees only
  the mosaic rectangle and pure black around it. The rectangle TRACKS the hihat
  sway (`dance_x`) so it stays flush with the mosaic's moving edge — a static
  rectangle would leak a beam sliver on one side or shave the puzzle on the
  other. World->screen is 1:1 at the quad depth (CAM_F == CAM_D), so the edge is
  `CX_PX + dance_x +/- qw/2`; edges are rounded outward to the rasterizer's
  pixel-center convention.
- The beams live BEHIND the puzzle: the pieces depth-test in front of them
  (§12), and where the revealed back texture is cut out (§14) the beams show
  through. They are also visible in the black letterbox margins on each side.

## 16. CyberPuzzle background — mesh pyramids
- Drawn in the SAME background layer as the laser beams, right after them and
  before `fill_surround_black`: `draw_background_pyramids(img, background_local)`.
  `background_local = st - beam_start` is the SAME elapsed time the beams use, so
  the pyramids also begin only once the 180-degree flip is COMPLETE (`t < 0` ->
  return, nothing drawn during fly-in / assembly / jiggle / flip).
- Geometry: wireframe 4-sided pyramids (apex + square base, 8 edges). Each one
  tumbles about its OWN origin via `rot3(yaw,pitch,roll)`, then is scaled,
  placed at its world centre and projected with `project` (the same camera the
  pieces use).
- Everything is HASHED from the pyramid index (`hash01`): depth behind the quad
  plane (`BG_PYRAMID_DEPTH_MIN..MAX`), apparent on-screen radius / speed / height,
  the tumble rates + phases, the colour (RED or ORANGE) and the brightness gain.
  Deterministic, no wall-clock RNG, like the beams.
- LOOPING (right -> left): world-x = `half_span - (speed*t + phase) mod
  2*half_span`, with `half_span` = a full screen width + the pyramid's own radius
  + a margin. So a pyramid rides from just OFF the right edge to just OFF the left
  edge, then wraps and slides back IN from beyond the right edge — it re-enters
  gradually, it does not pop in mid-screen.
- Blending: ADDITIVE over the black background (`draw_line_additive`, a soft glow:
  full weight on the core pixel, 0.30 on the 4 edge neighbours, 0.12 on the 4
  corners), so the wires read as glowing neon red/orange. Additive is
  order-independent, so no depth test is needed and the pyramids never write depth
  (they sit behind everything and are occluded by the pieces the normal way).
- VISIBILITY: being in the background layer, the pyramids are governed by the
  same two rules as the beams — `fill_surround_black` keeps them out of the black
  letterbox margins, and past the flip they are seen ONLY through the transparent
  cut-out region of the revealed back texture (`tex_scene4`). Note that texture's
  transparent area is concentrated on its RIGHT half (bands L->R: 0,0,0,0,0,0.25,
  0.73,1.0,1.0,1.0), so most of the pyramid field shows through the right side of
  the mosaic; that is the texture's alpha map, not a bug.
- COUNT: `BG_PYRAMID_COUNT = 14` — chosen so roughly 3-5 are inside the visible
  transparent band at any instant (the loop span is ~1.3 screens wide and only
  ~0.2 of a screen is see-through, so ~1/6 of them are visible at a time).
