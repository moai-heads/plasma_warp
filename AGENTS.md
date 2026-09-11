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
