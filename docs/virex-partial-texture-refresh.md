# Partial texture refresh correctness

Related to the black or missing textures reported in #89 and #264.
The separate relay dependency fixes in #191 and #207 are not duplicated.

A guarded texture can observe new writes, update its journal cursor,
then skip a partially mapped read because of the 30-frame polling
throttle. This delays known changes. Treat confirmed new writes as
a reason to bypass the partial polling throttle. Unchanged polling
remains throttled; reset the partial poll timestamp after a fully
mapped read path.

A partial all-zero read is intentionally not detiled, but used to
replace the cached hash anyway. Keep the previous staged hash for
this rejected read. Otherwise a subsequent fully mapped all-zero
texture can incorrectly compare equal to bytes never staged.

Two focused Rust regression tests cover the throttle and rejected
hash transitions. The patch does not alter the write-watch branch,
the 8-frame guard hash policy, GPU synchronization or detiling.
A frequently changing partially mapped texture may now be read
more often, trading CPU work for timely updates.

Native Windows/Rust and image correctness tests are pending.
The link to character skin artifacts is a hypothesis, not a verified
fix. No FPS gain is claimed. No game data or new dependencies.
AI-assisted (ChatGPT).
