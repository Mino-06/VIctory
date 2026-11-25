# Next steps

Work left on `integration/nvidia-first-launch-and-open-prs`, highest value first.

## Code

1. **upstream #1890, the useful part**: port `split.rs` (`convergent`,
   `post_dominators`) and an `lds_sync`-style fallback where `lds_meetings`
   returns `None` (crates/victory-shader/src/emit/lower.rs, the wave choice in
   `Lower::new`), behind an env switch. Target: the 4 structure-builder
   programs that still need `WaveSize(64)` on 32-lane hosts.
2. **upstream #1845, remaining parts**: dword 1 stride and dword 5
   `BASE_ARRAY` reconstruction in `descriptor_data`; untracked masks
   (`v_cndmask_b32` with inline 0/-1 or NULL masks, then `select_mask`).
3. **Shader translation cache misses on failure**: translations that fail
   are not cached, so every launch translates them again. Caching the
   failure (with its reason, logged on every hit) per build key would cut
   startup work without hiding the error.
4. **Pipeline workers during loading**: PR #294 keeps 2 workers until the
   world draws; with every created pipeline now prewarmed, measure whether
   more workers during loading screens shorten the first launch without
   uneven frames.
5. Remaining conflicting fork PRs worth a hand merge: #309-#400 (geometry
   shader series), #258 (FidelityFX programs), #261 (adapter choice),
   #224/#353 (shader lowering), #237 (library save serialisation; check
   against PR #211 first).
6. Skip: #280 (blend part duplicates PR #51; FP16 interpolation largely in
   PRs #52/#203), duplicates of merged PRs (see NOTES.md).

## Publishing

The GitHub account in this session (Mino-06) has pull access only to
fxceless/victory-fork. To publish, from an account with push access:

    git push -u origin integration/nvidia-first-launch-and-open-prs

or fork the repository and push there, then open a pull request to
fxceless/victory-fork `main`.
