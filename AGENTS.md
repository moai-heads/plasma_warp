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
- Direct `rustc` only — Cargo is forbidden for project development.
  See /root/POLICY/rust.md. Use the shared rlib vault at /root/rustlib/.
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
