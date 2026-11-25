# Overnight work log (integration/nvidia-first-launch-and-open-prs)

Resume state for anyone (or any agent) picking this up. Test machine:
Windows 10 Pro 19045, NVIDIA RTX 3090, VS 2022 Build Tools, D3D12 Agility
SDK 1.620.2 in `target/<profile>/D3D12` (`cargo xtask agility`).

Build with MSVC from PowerShell (Git Bash's `link` shadows MSVC's).

## Integrated (beyond the 155 clean merges of the first pass)

| PR | How | Notes |
|----|-----|-------|
| #201 | hand-resolved | UAV barrier skipped on textures without UAV access |
| #294, #340, #318, test-only shader PRs | resolved | |
| #363 | partial | pipeline-library mesh refusal + shadow split kept; copy path dropped (main's VEH copy, #154, already does it) |
| #346 | hand-resolved | copy marks; merged with #370's per-list taking; new copies count as marked |
| #220 | hand-resolved | indirect windows 47.8 -> 1.15 ms GPU on the 3090 (PR's own bench) |
| upstream #1843 | ported | typed-store check in `fits()` |
| #387, #377, #354 | hand-resolved | wave32 lane masks, implicit reads |
| #397 | hand-resolved | packed mesh outputs; `packed: false` elsewhere |
| upstream #1845 | partial port | FreshTables |

## Bugs found and fixed

- First launch: no prewarm, early list lost on crash (pipeline/make.rs, library.rs).
- `v_permlane16/x16`: PR #40's full lowering unreachable behind main's simple
  arm; PR #340's arm refused every non-zero selector. 6 permlane tests failed.
- Duplicate `v_cvt_f32_f64` arm.
- Shader examples failed to link once the Agility SDK was present.
- Repository policy test failed (NOTICE file, 584 KB lower.rs).
- PR #201's RGB9E5 test contradicted PR #32's FP16 stand-in.
- Indirect GPU tests crashed in parallel on the shared WARP device (pre-existing).

## Studied, not taken

- upstream #1868: covered by `target_format` stand-ins (PR #32) + PR #201.
- upstream #1845 wave32 compute, heap budget: already in the fork (see audit).
- upstream #1890 full port: depends on upstream lowering infrastructure.
- Duplicates of merged PRs (f16 rounding x6, XGameSave x6, jobs 0 x4, ...).

## Next

See NEXT_STEPS.md. Rejected: #280 (duplicates #51/#52/#203).

## Old next list

1. Wave32 correctness PRs #354, #377, #387 (NVIDIA).
2. #397 mesh 16-bit outputs (71 mesh shaders over 32 KiB).
3. #280 FP16 interpolation / integer RT PSO creation.
4. upstream #1845 FreshTables (draw.rs:188, 2026-2065), dword 1 stride /
   dword 5 BASE_ARRAY in descriptor_data.
5. upstream #1890 split.rs + lds_sync fallback at lds_meetings == None.
