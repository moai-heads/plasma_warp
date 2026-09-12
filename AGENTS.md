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
    `rustc --edition 2021 -O src/main.rs --extern image=/root/rustlib/libimage.rlib -L dependency=/root/rustlib -o app`
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
  downloads `sdl3`, `lewton`, `image` and their deps from crates.io. Fine.
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
