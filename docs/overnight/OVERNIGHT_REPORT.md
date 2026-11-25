# Overnight report: integration/nvidia-first-launch-and-open-prs

Machine: Windows 10 Pro 19045, NVIDIA RTX 3090. Every result below was run
on it. The title was **not** run (see "Not done").

## Improvements and bugs fixed

| Problem | Fix |
|---|---|
| A first launch prewarmed no pipeline, and the list of pipelines to prewarm was lost when a run crashed, so each relaunch dropped fewer draws | Every pipeline the title creates is queued during loading, behind known ones (`VICTORY_PREWARM_ALL=0` turns it off); the list is saved every 2 s instead of every 20 s |
| The pipeline library stored mesh pipelines it then refused to load: recompiled every run, library grew | PR #363: a kind the library refuses is no longer stored |
| Frames of 3+ s: every list declared all 1,023 title-memory copies (4.3 GB) written | PR #346, merged with PR #370: only copies shaders mark, the dispatch names, or that are new |
| ~3,200 indirect draws a frame each followed by a barrier | PR #220: windows of 32 went from 47.81 ms to 1.15 ms of GPU time (PR's benchmark, RTX 3090) |
| `v_permlane16/x16`: the complete lowering (PR #40) could never run; PR #340 refused every non-zero selector | Old arm removed, #340's arm guarded to its exact case; 6 failing tests now pass |
| Unordered views in formats NVIDIA cannot store to removed the device | Port of upstream #1843 (typed-store check) |
| Each binding of a shorter prefix of a table took new dynamic descriptor slots | Port of upstream #1845 `FreshTables` |
| Wave32 correctness on NVIDIA | PRs #354, #377, #387, #318, #340 merged and reconciled |
| Mesh shaders over the 32 KiB output limit (71 programs) | PR #397 merged |
| Duplicate `v_cvt_f32_f64` arm; Agility SDK link failure in shader examples; repository policy test failing; flaky crash of the parallel WARP tests | Fixed |

Why AMD ran better than NVIDIA, from the code: NVIDIA has 32-lane waves
only (wave64 programs refused), refuses typed stores to some formats
(B5G5R5A1, RGB9E5: device removed), caps the view heap at 1,000,000, and
Windows 10 without the Agility SDK has no shader model 6.6 (no mesh
shaders). The fork already had stand-in formats and the heap split; this
branch adds the typed-store check and the wave32 PRs.

## PRs

- Fork: 155 merged without conflict in the first pass, then by hand:
  #201, #294, #318, #340, #355 and the test-only shader PRs, #363 (partial),
  #346, #220, #387, #377, #354, #397.
- Upstream: #1843 ported, #1845 FreshTables ported, #1868 already covered,
  #1890 studied (only `split.rs` worth porting, see NEXT_STEPS.md).
- Not taken: #280 (duplicates PRs #51/#52/#203), duplicates of merged PRs.
  Details in NOTES.md.

## Tests

`cargo test --release --workspace` on the RTX 3090, WARP and the CPU:
- start of session: 681 passed, **8 failed**
  (`docs/overnight/tests-1-baseline.txt`)
- end: **705 passed, 0 failed**, 15 ignored
  (`docs/overnight/tests-2-final.txt`)

New tests: pipeline queue order, typed-store check, FreshTables, policy size
limits. Indirect GPU tests: 10 of 10 parallel runs clean (3 of 5 crashed
before).

## Not done

- The title was not run: the files on this machine come from a leaked
  build, so no launch, cold/warm start or frame-time measurement was made.
  The first-launch fix is checked by tests of its queue, not in the title.
- Push and pull request: the GitHub account here has read access only.
  See NEXT_STEPS.md.

## 2026-11-25: NEXT_STEPS items 3, 2 and 1 (branch claude/nvidia-next-steps-e4ce0b)

| Item | Commit | What changed |
|---|---|---|
| 3. Cache translation failures | 57f19fe | A failed translation is cached with its reason (`VSF1` entry) under the same build key as a success; every hit logs the reason as a warning and returns it as the error. `VICTORY_SHADER_CACHE_FAILURES=0` ignores those entries and translates again |
| 2. Upstream #1845, stride and BASE_ARRAY | 4bfca15 | `descriptor_data`: buffer dword 1 = stride at bits 16-29 where every read discards bits 0-15 (upstream's `only_high_bits_read`); 1D/2D array image dword 5 = 0. The fork's host view address is still used where the code reads the whole dword. Refusals name the part read (base address, format/metadata, unused slot) |
| 2. Upstream #1845, untracked masks | 7b42f3f | `v_cndmask_b32` and `v_*_co_ci_u32` take inline 0 / NULL (no lane) and -1 (every lane). The rest of upstream's commit (uniform `s_cselect`, constants, split wave64 halves, compare/constant joins) already worked in the fork's structure pass; the new test checks all of them on 32- and 64-lane hosts |
| 1. Upstream #1890 `split.rs` | d03ab87 | `convergent` / `post_dominators` ported (`emit/split.rs`). `VICTORY_WAVE_LDS_SYNC=1` (off by default, part of the cache key): where `lds_meetings` refuses a one-wave group, split it anyway, the waves meeting around the regions and before LDS accesses in blocks both analyses call together; inside the region the halves stay unordered |

Tests: `cargo test --release --workspace --no-fail-fast` on the RTX 3090:
Log: `docs/overnight/tests-3-next-steps.txt`. 705 -> 709 (items 3, 2a) -> 710 (2b) -> 712 (item 1) passed, 0 failed, 15 ignored.

Left / not checked:
- `VICTORY_WAVE_LDS_SYNC=1` was not tried on the 4 structure-builder programs:
  that needs the title, which was not run (leaked build). Turning it on by
  default needs that check first: the shortcut (no ordering inside a
  region) can be wrong.
- Upstream #1890's split lowering itself (`exchange`, `split_ballot`,
  ...) was not ported: the fork has its own (`exchange`, `lds_meetings`).
- `VICTORY_WAVE_LDS_ASSUME` changes translations but is not in the shader
  cache's build key (pre-existing).
- A fresh worktree needs the Agility SDK copied to `target/release/D3D12`
  **before** the first build of `victory-shader` (its build script decides
  then), or the GPU tests fail with "SM 0x65 only".
