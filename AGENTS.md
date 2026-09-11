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
