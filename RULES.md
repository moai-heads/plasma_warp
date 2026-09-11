# plasma_warp — Project Rules

Binding conventions for anyone (human or agent) working in this project.
These override default behaviour. Read before contributing.

## 1. Communication in chat
- **NEVER paste source code into a Discord message.** No full files, no large
  snippets, no "here's the full source" dumps. It turns the thread into a
  slop salad.
- Source code is delivered **only as a file attachment** (send the actual
  `main.rs` / script file). If a short excerpt is genuinely needed to explain
  a point, keep it to a few lines inline — never a whole file.
- Same rule for other large text: sync data, logs, build output — attach a file,
  do not inline it.

## 2. Renders and deliverables
- Generated artifacts (frames, mp4, gif, intermediate files) are deliverables
  only. Send them, then DELETE them from the project folder. Keep only source,
  config, and small persistent assets (input textures, sync labels).

## 3. Build
- Direct `rustc` only. Cargo is forbidden for project development (see
  /root/POLICY/rust.md). Use the shared rlib vault at /root/rustlib/.

## 4. Sync data
- `beats.txt` (snare) and `kicks.txt` (kick) are hand-labelled ground truth.
  Do not regenerate them with detection code. If sync is wrong, fix the labels.

## 5. Verification
- Every deliverable is verified before it is sent (frame count, duration,
  non-silent audio, and that the render actually differs as intended).
  No "it's fixed" claims on unverified builds.
