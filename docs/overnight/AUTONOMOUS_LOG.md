# Autonomous work log

Chronological record of what was tried, why, and what came of it.
Branch: `integration/nvidia-first-launch-and-open-prs`.

## 2026-11-25, pass 0: baseline

- `cargo test --release --workspace`: 705 passed, 0 failed, 15 ignored.
- `victory doctor`: READY (RTX 3090, shader model 6.6, mesh shaders, 32-lane waves only).
- Created `victory.toml` (git-ignored) with `game_dir = 'D:\Games\GT2\GT XBUILD'`.
- First launch: `victory run --execute D:/Games/GT2/carlo/game.exe`.
  - Bound 89 HLE, 427 host, 71 missing imports (XboxBvhBuilder 7,
    amd_fidelityfx_loader_xbox 3, libHttpClient.GDK 38, Bink2Scarlett 23).
  - `rpcrt4.dll` refused by the host DLL policy.
  - Virtual drive G rooted at `D:/Games/GT2/carlo` (the exe's folder), which
    holds only `game.exe`; the data (`*.rpf`) is in `GT XBUILD`.
  - 0.3 s after start: exception 0x80000003 (int3) at image+0x2237178, the
    title's own crash reporter then ran. No window, no frame.
- Session `nvidia-fixes` (own worktree, branch `nvidia-next-steps`) started
  on NEXT_STEPS items 3, 2, 1 (cache failed translations, #1845 rest, #1890
  split.rs).

## Audit findings

### Performance (done)

1. Failed shader translations are not cached: retranslated every launch
   (`crates/victory-gpu/src/shader_cache.rs`). Owned by session `nvidia-fixes`.
2. Pipeline workers above the 2 menu workers poll with a 50 ms sleep until
   the world draws (`crates/victory-gpu/src/pipeline/make.rs`, about line 381).
   A condvar would wake them at once.
3. `VICTORY_INDIRECT_WINDOW` is off by default (`crates/victory-gpu/src/draw.rs`,
   about line 146) although PR #220 measured 47.81 ms -> 1.15 ms.
4. `FreshTables` does a linear search per table binding (`draw.rs`, about line 215).
5. Root SRV/UAV slots are `HashMap` lookups per draw (`draw.rs`, about line 198).

### Crash and HLE coverage

The audits proposed stub modules for the 71 missing imports and turning
`VICTORY_SKIP_TITLE_INT3` on. Neither was needed: see pass 1.

## Pass 1: the title starts

- **Cause of the startup crash**: `run` took the exe's folder as the package
  folder. `carlo/` holds only `game.exe`, so the title's own DLLs
  (XboxBvhBuilder, Bink2Scarlett, libHttpClient, FFX loader) were not found,
  their 71 imports trapped, and the title's crash reporter hit its int3.
- **Fix** (`apps/victory/src/run.rs`): when the exe's folder has no
  `MicrosoftGame.config` and the configured `game_dir` has one, `game_dir` is
  the package (DLLs, G: drive, manifest). Result: 4 title DLLs mapped,
  0 missing imports, no exception; the title reaches its profile screen
  ("SELECTEUR DE PROFIL (Y) / PLAYER (A)") at 30 fps, 13 draws/frame.
- **Window placement** (`crates/victory-hle/src/modules/lifecycle.rs`):
  `VICTORY_WINDOW_AT=X,Y` puts the title's window on the screen nearest that
  point (fitted to its work area); `VICTORY_WINDOW_QUIET=1` creates it with
  `WS_EX_NOACTIVATE` and gives the keyboard back, so test runs on the second
  screen never take focus. Checked: 5 of 5 samples per run not foreground.
- **Log noise** (`pipeline/host.rs`): "host pipeline" per pipeline made or
  loaded went to debug: 50,468 of 53,514 lines of a 3-minute run.
- **Pipeline workers** (`pipeline/make.rs`): workers past the menu count wait
  on a condvar instead of a 50 ms sleep poll; `VICTORY_MENU_WORKERS=N`
  overrides the 2 menu workers. Measured, 2 vs 6: same 30 fps in the
  profile screen; queue 9,434 vs 9,262 left after 2 min (the queue is held
  by a few huge driver compiles, not by worker count). Default kept at 2.
- **Diagnosis logs**: "slow host pipeline" (>= 1 s, with shaders) and
  "slow shader translation" (>= 1 s). Found: ~90 compute pipelines per run
  take 6-52 s each in the driver (`CreateComputePipelineState`); the three
  worst 49-52 s (compute 1250dc0656b3c091, ffd9788ba9fec033, fb515ca95d694c18).
  The same shaders are slow every run because most of them never finish
  before the run ends, so they never reach the pipeline library.
- Tests: 705 passed, 0 failed, 15 ignored.
- Mistake caught: a `cargo fmt --all` reformatted ~75 untouched files; all
  reverted, only the edits above kept.

Session `nvidia-fixes` finished on its own branch
`claude/nvidia-next-steps-e4ce0b` (not merged here): failed translations
cached, #1845 dword 1/5 and masks, #1890 `split.rs` behind
`VICTORY_WAVE_LDS_SYNC` (off). Its suite: 712 passed, 0 failed.

## Pass 2: through the menus to the prologue

- Diagnostics build kept apart in `target-diag/` (git-ignored).
- `VICTORY_KEYS` takes an optional hold time, `<s>:<key>:<held>` (default
  0.2 s), so a stick can be held to walk or drive (W/A/S/D left stick,
  I/J/K/L right, Z/C triggers, Enter A, Backspace B, arrows d-pad).
- Scripted path that works (times from start, they drift a few seconds
  with loading): Enter 25, 32, 45, 60 (profile, subtitles, brightness);
  Down-Down-Enter every 10 s from 80 to 140 (legal notices: "Suivant");
  then Up, Up, Enter, Down, Enter, Down, Enter at 152-158 (tick privacy
  charter, tick terms of use, Confirmer). Kept in `vshot/keys.txt`.
- After Confirmer (~150 s) the prologue loads: 26,000 pipelines queued,
  world at 0.5-1 fps, 400-1,000 draws dropped per frame (pipelines not
  ready), title window unresponsive.
- **Memory** (run nav4 memory line): 26.7 GB private, 7 GB of it resident,
  15 MB free on the PC: the machine pages. Of it, 3.4 GB were texture
  staging buffers alive, only 191 kept by their texture.
- **Bug fixed** (`crates/victory-gpu/src/host.rs`, `Deferred::clear`): a
  list reset cleared everything but `texture_uploads`. Each recording
  queued its uploads again, so every staging buffer stayed alive for the
  list's whole life and every prelude walked an ever longer list.
- Not a bug: "library saved bytes=0" is the 2 s early-list save between the
  20 s library saves.

- Measured after the fix (run nav5 vs nav4, same keys): texture staging
  alive 185 MB vs 3,021 MB; prologue 1.1-1.3 fps vs 0.5-0.8 fps. Still
  ~23 GB private and ~1 GB free on the PC (other applications hold the
  rest): the next limit is RAM and pipelines being made (~25,000 queued).
- `--console series-s` (run ss1): 17 GB private instead of 23, same fps,
  and the title asserts (int3 at image+0x23d8f03, on a title thread) early
  in the prologue. Not usable; stay on Series X.
- Captures: the title's window is found by enumerating the process's
  windows (`shot2.ps1`), and a hung window is flagged.

## Pass 3: parallel work

- Session `shader-gaps` (branch `shader-gaps`): shaders that do not
  translate (43-65 per run), offline through `victory translate`.
- Session `slow-compiles` (branch `slow-compiles`): compute pipelines of
  6-52 s in the NVIDIA driver, offline with a timing harness.
- This session: in-game runs only (one title at a time), texture bugs at
  the end of the prologue cinematic, memory, stability.

## Pass 4: the prologue crash

- Ollama (server + tray app that restarts it) stopped at the user's
  request; its Startup shortcut moved to `%USERPROFILE%\startup-disabled\`.
- Runs ss1 (Series S) and cin1 (Series X) both ended with exit 3: the
  title's int3 at image+0x23d8f03, on a title thread, ~3.5 min in. Just
  before it, in both: two DirectStorage plain copies (1.4 KB each) failed
  with 0x800706F8 (ERROR_INVALID_USER_BUFFER): the kernel would not write
  the title's destination. The title asserts on the failed read.
- Fix (`crates/victory-hle/src/modules/dstorage.rs`): on that error the
  request is read into a buffer and copied by the CPU, through the GPU
  guard's fault handler. Logged as "a read landed through a buffer".

- Verified (run cin2, 8 min): 10 refused reads landed through the buffer,
  no crash. Committed (6 commits) and merged `nvidia-fixes`: 712 passed.

## Pass 5: where the time goes (25-min warm-up run)

- Host: CPU 100 %, GPU 14 % (RTX 3090 at 169 W, 12.6 of 24 GB): the
  title is CPU-bound in the emulator, not GPU-bound. A session's offline
  `translate --jobs 6` took ~9 cores; set to idle priority, both sessions
  asked to run low-priority with 2 jobs.
- Prologue now 1.7-2.0 fps (from ~1), 9,500 pipelines queued (from 25,000):
  the pipeline library fills from run to run (68,000 loaded, 2,700 early).
- **2,718 pipelines took >= 1 s, 8,630 s of worker time** (p50 2.3 s,
  p90 6.0 s, max 48 s): nearly all of the workers' time.
  - mesh+pixel: 1,978, 6,630 s, avg 3.4 s; our translation ~450 s of it:
    the NVIDIA driver compiling mesh pipelines is the main cost.
  - compute: 633, 1,771 s; our translation 1,663 s of it: the translator.
  Sent to session `slow-compiles`.
- Draws whose pipeline is not ready are dropped (~400 per frame): this is
  the "world loading behind, things overlapping" in the cinematic.
- Per-frame preparing 150-420 ms, of it copies of title memory 150-320 ms
  for 100-250 MB (~0.7 GB/s) and ~2,800 mapping changes per frame
  (126 ms). Direct copy is already on.
- The shader cache is keyed by our executable's hash: every rebuild
  retranslates everything once. A/B runs therefore share one binary.
- A/B chain (detached, `vshot/chain.sh`, results in `vshot/ab.txt`):
  base, VICTORY_INDIRECT_WINDOW=1, VICTORY_LATE_WAIT=1,
  VICTORY_WIDE_MERGE=1, VICTORY_JOIN_COPIES=1.

## Pass 6: white and blocky screens

- User's screenshots (prologue): over-exposed white frames, large blurred
  rectangles, stale tiles. Matches dropped compute passes (exposure,
  bloom, tiled lighting, post), not missing textures: the "3,012 title
  textures have no host copy yet" are textures not used yet (made on
  first use).
- Two causes of dropped dispatches:
  1. 43 compute pipelines the host refuses (`CreateComputePipelineState`
     E_INVALIDARG), dropped every frame. List in
     `docs/overnight/refused-compute-pipelines.txt`; given to session
     `shader-gaps` to reproduce offline with the debug layer and fix.
  2. Compute pipelines still being made after the shared 500 ms budget.
- Change (`pipeline/make.rs`): a dispatch whose pipeline is not ready waits
  on a budget of its own per frame (3 s, `VICTORY_COMPUTE_WAIT_MS`, 0 = off)
  after the shared one. A dropped dispatch spoils the whole frame, a dropped
  draw one object. To be measured (chain2: prime, cw0, cw3 with captures).

## Pass 7: starved pipeline workers

- Session `slow-compiles`: the slowest mesh+pixel pairs compile ~10x faster
  offline than in game (4408698e+224c743d: 4.2-5.1 s vs 48.2 s), on the same
  translation path; the driver does not serialise, memory is not it. The
  in-game time is waiting for the CPU: 8 workers at below-normal priority
  under a saturated CPU.
- Changes (`pipeline/make.rs`): a worker making a pipeline a draw or
  dispatch waits for now runs above normal priority, the rest (early,
  speculative) below normal as before; "slow host pipeline" also logs the
  thread's CPU time (`cpu`), so waiting shows as `cpu` far below `ms`.
- A/B so far (420 s runs, means over world frames):
  - base: 1.28 fps, prep 241 ms, copies 121, GPU 129, dropped 706/frame
  - VICTORY_INDIRECT_WINDOW=1: 1.60 fps (+25 %), prep 218, GPU 99
  - VICTORY_LATE_WAIT=1: 2.05 fps (+60 %), prep 190, GPU 62, copying back
    44; later in its world 2.6-3.4 fps. CPU and GPU now overlap.
  - VICTORY_WIDE_MERGE=1: not comparable. The run spent ~5 min on a loading
    screen (captures: artwork at frames 4096 and 8192) because the first
    world-frame capture switch saved nearly every frame (685 files) and
    slowed it. That switch is off again. Its frame 10095 shows the white
    over-exposure in the cinematic ("DU CASH, MAINTENANT").
- Winners INDIRECT_WINDOW and LATE_WAIT set as defaults for the next runs
  (`vshot/defaults.env`), to be confirmed on captured frames before making
  them the code's defaults. A/B lines now also give the loading time
  (start to first frame of 100+ draws; run late: 98 s).
- Window captures (PrintWindow) return stale frames for the title's
  flip-model window; runs now save the presented frames themselves
  (`VICTORY_CAPTURE`, frames 4000-7000 every 150, and the first world frame).

## Next

1. Press A automatically (diagnostics build, `VICTORY_KEYS`) to get past the
   profile screen and test the menus and the world.
2. The 50 s driver compiles: find what in the translated code makes them
   slow (size, loops, unrolled waves) and cut it.
3. 43-51 shaders that do not translate (SGPR address not a root parameter,
   whole-wave sections, lane masks, 16-bit wave sums).
4. 29 kinds of pipeline creation method not implemented (slots 0x218, 0x1c8).

## Pass 8: state when the usage limit was reached

- Run join (VICTORY_JOIN_COPIES=1) never reached the world in 420 s, and
  widemerge loaded for ~5 min: both had `VICTORY_CAPTURE` on, which saved
  nearly every frame (806 files) and stalled loading. Capture is off again;
  WIDE_MERGE and JOIN_COPIES remain unmeasured.
- Committed: compute dispatches wait on their own 3 s budget
  (`VICTORY_COMPUTE_WAIT_MS`), workers making a pipeline a draw waits for run
  above normal priority, slow pipelines log thread CPU time. Pipeline module
  tests pass (21); full suite not rerun after these.
- Running unattended: chain2 (`vshot/chain2.sh`) rebuilds the diagnostics
  binary and runs prime2, cw0, cw3 with INDIRECT_WINDOW=1 and LATE_WAIT=1;
  results append to `vshot/ab.txt`.
- Sessions still working on their branches (not merged): `shader-gaps`
  (untranslatable shaders, 43 refused compute pipelines),
  `slow-compiles` (mesh DXIL size, translator speed).

## Left to do

1. Read `ab.txt` (prime2, cw0, cw3); confirm INDIRECT_WINDOW and LATE_WAIT
   on in-game frames, then make them defaults in code.
2. Run the full test suite; merge the two session branches when done.
3. White over-exposure in the cinematic: likely the refused compute
   pipelines (exposure); check after shader-gaps lands.
4. Go past the prologue: script movement (W held, look, run) and long runs.

## Pass 9: session branches merged (not yet tested together)

- Merged `claude/shader-gaps-95823e` (compute shaders failing to translate
  in wave-32 mode: 65 -> 10; the 43 refused pipelines now fail translation
  with their real reason, still dropped) and `claude/slow-compiles-598521`
  (mesh compile 2-4 % faster, `translate --pipelines` timing harness).
- chain2 prime2 (INDIRECT_WINDOW + LATE_WAIT, new binary, retranslating):
  2.05 fps, load 134 s, GPU 43 ms/frame, no crash. cw0/cw3 still running.
- TODO first: `cargo test --release --workspace` on the merged tree (the
  shader-gaps worktree could not run 39 on-card tests).

## Pass 10: compute wait verified, merged tree tested

- chain2 (INDIRECT_WINDOW + LATE_WAIT on, warm cache): cw0 (compute wait
  off) 2.87 fps, load 84 s; cw3 (3 s compute wait, the default) 2.95 fps,
  load 83 s. The wait costs nothing in frame rate: kept on.
- Merged tree: `cargo test --release --workspace` 715 passed, 0 failed.
- The 43 refused compute pipelines (session shader-gaps): no exact 32-lane
  form, so the 64-lane original with `WaveSize=64` reached the driver,
  which the RTX 3090 refuses. They now fail translation with that reason.
  `VICTORY_WAVE_LDS_SYNC=1` makes 40 of 43 create, approximately (a
  half-wave may read LDS before the other wrote it). It is part of the
  shader cache key. Queued: chain4 (prime, then a measured run with
  captures) to judge it on screen.
- Captures are now on-screen copies of the title's window (it sits on top
  of its screen); `PrintWindow` gave stale frames. chain3: vis_on / vis_off
  for a visual check of INDIRECT_WINDOW + LATE_WAIT.
- Found by shader-gaps, not fixed: `lds_lane_private` (`lower.rs`) treats
  some LDS accesses as private per lane when they are not (neighbour reads
  like `lds[t+1]`, uniform addresses); ~117 lowerings run as two 32-lane
  waves with no ordering. Notes in the session's `shader-gaps.md`.

## Pass 11: the prologue renders right with a warm cache

- chain3 after rebuilding the diagnostics binary (every shader translated
  again on the first run): vis_on (INDIRECT_WINDOW + LATE_WAIT) ran first,
  cold: 1.27 fps, 1,359 draws dropped/frame, white over-exposure at
  360-390 s. vis_off ran second, warm: 1.81 fps, 689 dropped, and the
  cinematic is **correct**: exposure, prison yard, character and hair
  (`docs/overnight/vis-on-off-cold-warm.png`, top on/cold, bottom off/warm).
- So the white and blocky screens come mostly from passes whose pipeline
  is not made yet; they go once the caches are warm. The order biased this
  pair: chain5 repeats it warm (vis_off2 then vis_on2) to see whether the
  two switches themselves change the image.
- lds_prime (VICTORY_WAVE_LDS_SYNC=1, retranslating): 1.90 fps, no crash.

## Pass 12: control handed to the player

- lds (VICTORY_WAVE_LDS_SYNC=1 with INDIRECT_WINDOW + LATE_WAIT, warm):
  2.54 fps, load 81 s, no crash. vis_off2 (all three off, warm): 2.23 fps.
  Both render the cinematic correctly; LDS sync shows no visible artefact
  (`docs/overnight/lds-sync-vs-off.png`, top lds, bottom off).
- At ~360 s the game shows "Se déplacer / Bouger la caméra": the prologue
  hands control to the player. Queued chain6: a 600 s run with movement
  keys after 370 s (W held, camera J/L, Enter), captures every 20 s.
