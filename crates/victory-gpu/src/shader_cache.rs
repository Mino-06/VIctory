//! Translated shaders kept on disk (decision 0003): under the configured
//! cache folder, keyed by the container's content and by the VIctory build
//! that translated it (the BLAKE3 of our executable), so a rebuilt
//! translator never serves an old translation. Entries go through
//! `victory-pipeline`'s content-addressed cache, which writes them
//! atomically; an entry that does not read back is translated again.
//!
//! The entry is our own encoding of [`HostShader`]: a field added to it
//! must be added to `encode` and `decode` too (entries of older builds
//! are ignored anyway).
//!
//! A translation that fails is cached too, with its reason, under the same
//! key: the translation depends only on the container and on what the build
//! key holds, so it would fail the same way on every launch. Each hit on a
//! failed entry logs the reason again and returns it as the error, so the
//! failure is never hidden. `VICTORY_SHADER_CACHE_FAILURES=0` ignores those
//! entries and translates again (to see the translator's own logs).

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use victory_pipeline::{Cache, Converter};
use victory_shader::emit::{HostShader, StaticSampler, UserData};
use victory_shader::resource::RootIndexedView;
use victory_shader::{HwStage, RootType};

static CACHE: OnceLock<Cache> = OnceLock::new();
static HITS: AtomicU64 = AtomicU64::new(0);
static MISSES: AtomicU64 = AtomicU64::new(0);

/// Keeps translated shaders in `dir` from now on.
pub fn use_shader_cache(dir: PathBuf) {
    match Cache::open(dir) {
        Ok(cache) => {
            let _ = CACHE.set(cache);
        }
        Err(e) => tracing::warn!(error = %e, "no shader cache"),
    }
}

/// Shaders found in the cache and translated since the start.
pub(crate) fn counts() -> (u64, u64) {
    (HITS.load(Ordering::Relaxed), MISSES.load(Ordering::Relaxed))
}

/// What identifies this build: the BLAKE3 of our executable file and of
/// the translation settings it runs with (the loop limit, which a
/// diagnostics build can change at run time).
fn build() -> &'static [u8; 32] {
    static BUILD: OnceLock<[u8; 32]> = OnceLock::new();
    BUILD.get_or_init(|| {
        let exe = std::env::current_exe().and_then(std::fs::read);
        let limit = victory_shader::emit::loop_limit();
        if limit != victory_shader::emit::DEFAULT_LOOP_LIMIT {
            tracing::warn!(
                limit,
                "VICTORY_SHADER_LOOP_LIMIT: translated loops stop sooner"
            );
        }
        match exe {
            Ok(mut bytes) => {
                bytes.extend_from_slice(&limit.to_le_bytes());
                // So is the view heap's null slot (it differs by host GPU):
                // a translation cached without the clamp must not be served.
                crate::descriptor::host::probe_view_limit();
                bytes.extend_from_slice(&victory_shader::emit::view_heap_limit().to_le_bytes());
                // Which group barriers wait is part of the translation too.
                bytes.push(victory_shader::emit::group_sync());
                // So are the switches that pick the form of a wave64 program:
                // without them here a run with `VICTORY_WAVE_WIDE=0` was
                // served the translations an earlier run of the same
                // executable had cached with the switch unset.
                for name in [
                    "VICTORY_WAVE_WIDE",
                    "VICTORY_WAVE_DEEP_SKIPS",
                    "VICTORY_WAVE_LOCAL",
                    "VICTORY_WAVE_OWN_BITS",
                    "VICTORY_F16_NEAREST",
                ] {
                    bytes.extend_from_slice(name.as_bytes());
                    bytes.push(b'=');
                    if let Some(value) = std::env::var_os(name) {
                        bytes.extend_from_slice(value.as_encoded_bytes());
                    }
                    bytes.push(0);
                }
                // And which clean-up passes ran over the translated code.
                bytes.extend_from_slice(&victory_shader::emit::shader_opt().to_le_bytes());
                *blake3::hash(&bytes).as_bytes()
            }
            Err(e) => {
                // Without it no entry can be trusted: a fresh key per run.
                tracing::warn!(error = %e, "cannot read our executable; shader cache unused");
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                *blake3::hash(&now.as_nanos().to_le_bytes()).as_bytes()
            }
        }
    })
}

/// The translation of a console shader container, from the cache when
/// there is one.
pub(crate) fn host_shader(container: &[u8]) -> Result<HostShader, String> {
    let translate = |c: &[u8]| victory_shader::emit::translate(c).map_err(|e| e.to_string());
    match CACHE.get() {
        Some(cache) => cached(
            cache,
            build(),
            victory_shader::emit::host_uses_wave32(),
            container,
            translate,
        ),
        None => translate(container),
    }
}

/// [`host_shader`] for a pipeline whose mesh and pixel shaders are
/// translated together with `link`: an entry of its own, keyed by the
/// link too.
pub(crate) fn host_shader_linked(
    container: &[u8],
    link: &victory_shader::emit::Link,
) -> Result<HostShader, String> {
    let translate =
        |c: &[u8]| victory_shader::emit::translate_linked(c, link).map_err(|e| e.to_string());
    match CACHE.get() {
        Some(cache) => {
            let key = *blake3::hash(&[&build()[..], b"link", &link.key()].concat()).as_bytes();
            cached(
                cache,
                &key,
                victory_shader::emit::host_uses_wave32(),
                container,
                translate,
            )
        }
        None => translate(container),
    }
}

/// The cache entry's converter: the build, host wave mode and container in, the
/// encoded translation out. `ran` tells a miss from a hit.
struct Translate<F> {
    translate: F,
    ran: AtomicBool,
    /// Stores a failure as an entry of its own instead of returning it.
    keep_failures: bool,
}

impl<F: Fn(&[u8]) -> Result<HostShader, String> + Send + Sync> Converter for Translate<F> {
    fn id(&self) -> &'static str {
        "shader.dxil"
    }
    fn version(&self) -> u32 {
        // Version 1 did not distinguish translations made for 32-lane hosts.
        2
    }
    fn convert(&self, input: &[u8]) -> anyhow::Result<Vec<u8>> {
        self.ran.store(true, Ordering::Relaxed);
        match (self.translate)(&input[33..]) {
            Ok(shader) => Ok(encode(&shader)),
            Err(reason) if self.keep_failures => Ok(encode_failure(&reason)),
            Err(reason) => Err(anyhow::Error::msg(reason)),
        }
    }
}

/// Whether failed translations are cached (`VICTORY_SHADER_CACHE_FAILURES=0`
/// turns it off).
fn keep_failures() -> bool {
    static KEEP: OnceLock<bool> = OnceLock::new();
    *KEEP.get_or_init(|| {
        std::env::var("VICTORY_SHADER_CACHE_FAILURES").map_or(true, |v| v.trim() != "0")
    })
}

fn cached(
    cache: &Cache,
    build: &[u8; 32],
    wave32: bool,
    container: &[u8],
    translate: impl Fn(&[u8]) -> Result<HostShader, String> + Send + Sync,
) -> Result<HostShader, String> {
    cached_with(cache, build, wave32, container, translate, keep_failures())
}

fn cached_with(
    cache: &Cache,
    build: &[u8; 32],
    wave32: bool,
    container: &[u8],
    translate: impl Fn(&[u8]) -> Result<HostShader, String> + Send + Sync,
    keep_failures: bool,
) -> Result<HostShader, String> {
    let input = [&build[..], &[u8::from(wave32)], container].concat();
    let converter = Translate {
        translate,
        ran: AtomicBool::new(false),
        keep_failures,
    };
    for _ in 0..2 {
        let path = cache
            .get_or_convert(&converter, &input)
            .map_err(|e| format!("{e:#}"))?;
        match std::fs::read(&path).ok().and_then(|b| read_entry(&b)) {
            Some(Ok(shader)) => {
                let (counter, what) = if converter.ran.load(Ordering::Relaxed) {
                    (&MISSES, "translated and cached")
                } else {
                    (&HITS, "from the cache")
                };
                counter.fetch_add(1, Ordering::Relaxed);
                tracing::debug!(path = %path.display(), "shader {what}");
                return Ok(shader);
            }
            Some(Err(reason)) if keep_failures => {
                if converter.ran.load(Ordering::Relaxed) {
                    MISSES.fetch_add(1, Ordering::Relaxed);
                    tracing::debug!(path = %path.display(), %reason, "shader translation failed; failure cached");
                } else {
                    HITS.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        path = %path.display(),
                        %reason,
                        "shader translation failed in an earlier run (cached failure; VICTORY_SHADER_CACHE_FAILURES=0 translates again)"
                    );
                }
                return Err(reason);
            }
            // A failure an earlier run cached while failures are not kept
            // now, or a damaged entry: removed below, then translated again.
            Some(Err(_)) | None => {}
        }
        tracing::debug!(path = %path.display(), "unreadable shader cache entry; translating again");
        let _ = std::fs::remove_file(&path);
    }
    MISSES.fetch_add(1, Ordering::Relaxed);
    (converter.translate)(container)
}

const MAGIC: &[u8; 4] = b"VSH1";
/// A failed translation: this tag, then the reason in UTF-8.
const FAILED: &[u8; 4] = b"VSF1";

fn encode_failure(reason: &str) -> Vec<u8> {
    [FAILED.as_slice(), reason.as_bytes()].concat()
}

/// A cache entry: a translation, or the reason one failed. `None` for a
/// damaged entry.
fn read_entry(bytes: &[u8]) -> Option<Result<HostShader, String>> {
    match bytes.strip_prefix(FAILED.as_slice()) {
        Some(reason) => String::from_utf8(reason.to_vec()).ok().map(Err),
        None => decode(bytes).map(Ok),
    }
}
const STAGES: [HwStage; 8] = [
    HwStage::Ls,
    HwStage::Hs,
    HwStage::Es,
    HwStage::Gs,
    HwStage::Vs,
    HwStage::Ps,
    HwStage::Ms,
    HwStage::Cs,
];

fn root_code(r: RootType) -> u32 {
    match r {
        RootType::Constants => 0,
        RootType::Descriptor => 1,
        RootType::Table => 2,
        RootType::ImmediateSampler => 3,
        RootType::ResourceHeap => 4,
        RootType::SamplerHeap => 5,
        RootType::Other(x) => 6 + x,
    }
}

fn root_type(code: u32) -> RootType {
    match code {
        0 => RootType::Constants,
        1 => RootType::Descriptor,
        2 => RootType::Table,
        3 => RootType::ImmediateSampler,
        4 => RootType::ResourceHeap,
        5 => RootType::SamplerHeap,
        x => RootType::Other(x - 6),
    }
}

/// Little-endian words and length-prefixed lists.
fn encode(s: &HostShader) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    let word = |out: &mut Vec<u8>, v: u32| out.extend(v.to_le_bytes());
    let stage = STAGES.iter().position(|&x| x == s.stage).unwrap_or(0) as u32;
    word(&mut out, stage);
    word(&mut out, s.dxil.len() as u32);
    out.extend(&s.dxil);
    word(&mut out, s.user_data.len() as u32);
    for u in &s.user_data {
        for v in [u.index, u32::from(u.slot), root_code(u.root), u.dword] {
            word(&mut out, v);
        }
    }
    word(&mut out, s.samplers.len() as u32);
    for sampler in &s.samplers {
        for &v in sampler {
            word(&mut out, v);
        }
    }
    word(&mut out, s.static_samplers.len() as u32);
    for t in &s.static_samplers {
        let [u, v, w] = t.address;
        let words = [
            t.filter,
            u,
            v,
            w,
            t.mip_lod_bias.to_bits(),
            t.max_anisotropy,
            t.comparison,
            t.border,
            t.min_lod.to_bits(),
            t.max_lod.to_bits(),
        ];
        for v in words {
            word(&mut out, v);
        }
    }
    for slots in [&s.root_cbvs, &s.root_srvs, &s.root_uavs] {
        word(&mut out, slots.len() as u32);
        out.extend(slots);
    }
    word(&mut out, s.root_indexed_views.len() as u32);
    for view in &s.root_indexed_views {
        for value in [
            u32::from(view.table_slot),
            u32::from(view.constant_slot),
            view.constant_dword,
            view.byte_offset,
            u32::from(view.shift),
        ] {
            word(&mut out, value);
        }
    }
    out
}

fn decode(bytes: &[u8]) -> Option<HostShader> {
    struct Reader<'a>(&'a [u8]);
    impl<'a> Reader<'a> {
        fn take(&mut self, n: usize) -> Option<&'a [u8]> {
            let (head, rest) = self.0.split_at_checked(n)?;
            self.0 = rest;
            Some(head)
        }
        fn word(&mut self) -> Option<u32> {
            Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
        }
        fn list(&mut self) -> Option<Vec<u8>> {
            let n = self.word()? as usize;
            Some(self.take(n)?.to_vec())
        }
    }
    let mut r = Reader(bytes);
    if r.take(4)? != MAGIC {
        return None;
    }
    let stage = *STAGES.get(r.word()? as usize)?;
    let dxil = r.list()?;
    let n = r.word()? as usize;
    let mut user_data = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        user_data.push(UserData {
            index: r.word()?,
            slot: u8::try_from(r.word()?).ok()?,
            root: root_type(r.word()?),
            dword: r.word()?,
        });
    }
    let n = r.word()? as usize;
    let mut samplers = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        samplers.push([r.word()?, r.word()?, r.word()?, r.word()?]);
    }
    let n = r.word()? as usize;
    let mut static_samplers = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        static_samplers.push(StaticSampler {
            filter: r.word()?,
            address: [r.word()?, r.word()?, r.word()?],
            mip_lod_bias: f32::from_bits(r.word()?),
            max_anisotropy: r.word()?,
            comparison: r.word()?,
            border: r.word()?,
            min_lod: f32::from_bits(r.word()?),
            max_lod: f32::from_bits(r.word()?),
        });
    }
    let root_cbvs = r.list()?;
    let root_srvs = r.list()?;
    let root_uavs = r.list()?;
    let n = r.word()? as usize;
    let mut root_indexed_views = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        root_indexed_views.push(RootIndexedView {
            table_slot: u8::try_from(r.word()?).ok()?,
            constant_slot: u8::try_from(r.word()?).ok()?,
            constant_dword: r.word()?,
            byte_offset: r.word()?,
            shift: u8::try_from(r.word()?).ok().filter(|&s| s < 32)?,
        });
    }
    r.0.is_empty().then_some(HostShader {
        stage,
        dxil,
        user_data,
        samplers,
        static_samplers,
        root_cbvs,
        root_srvs,
        root_uavs,
        root_indexed_views,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shader() -> HostShader {
        HostShader {
            stage: HwStage::Ps,
            dxil: b"DXBC and the rest".to_vec(),
            user_data: vec![UserData {
                index: 8,
                slot: 1,
                root: RootType::Table,
                dword: 0,
            }],
            samplers: vec![[0x400, 0xc0_0000, 0x18f0_0000, 0]],
            static_samplers: vec![StaticSampler::from_words([
                0x400,
                0xc0_0000,
                0x18f0_0000,
                0,
            ])],
            root_cbvs: vec![2],
            root_srvs: vec![0, 3],
            root_uavs: vec![1],
            root_indexed_views: vec![RootIndexedView {
                table_slot: 1,
                constant_slot: 2,
                constant_dword: 3,
                byte_offset: 32,
                shift: 5,
            }],
        }
    }

    #[test]
    fn translations_come_back_from_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(dir.path()).unwrap();
        let build = [7u8; 32];
        let container = b"a console container".as_slice();
        let runs = AtomicU64::new(0);
        let translate = |_: &[u8]| {
            runs.fetch_add(1, Ordering::Relaxed);
            Ok(shader())
        };
        assert_eq!(cached(&cache, &build, false, container, translate), Ok(shader()));
        assert_eq!(cached(&cache, &build, false, container, translate), Ok(shader()));
        assert_eq!(runs.load(Ordering::Relaxed), 1, "the second came from disk");

        // Another build translates again.
        assert_eq!(cached(&cache, &[8; 32], false, container, translate), Ok(shader()));
        assert_eq!(runs.load(Ordering::Relaxed), 2);

        // A damaged entry is a miss, translated and stored again.
        for e in walk(dir.path()) {
            std::fs::write(e, b"VSH1 broken").unwrap();
        }
        assert_eq!(cached(&cache, &build, false, container, translate), Ok(shader()));
        assert_eq!(runs.load(Ordering::Relaxed), 3);
        assert_eq!(cached(&cache, &build, false, container, translate), Ok(shader()));
        assert_eq!(runs.load(Ordering::Relaxed), 3);

    }

    #[test]
    fn failures_are_cached_with_their_reason() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(dir.path()).unwrap();
        let runs = AtomicU64::new(0);
        let fail = |_: &[u8]| {
            runs.fetch_add(1, Ordering::Relaxed);
            Err::<HostShader, _>("unsupported opcode v_foo".to_owned())
        };
        let reason = Err("unsupported opcode v_foo".to_owned());
        assert_eq!(cached_with(&cache, &[7; 32], false, b"bad", fail, true), reason);
        // Hits return the same reason without translating again.
        assert_eq!(cached_with(&cache, &[7; 32], false, b"bad", fail, true), reason);
        assert_eq!(cached_with(&cache, &[7; 32], false, b"bad", fail, true), reason);
        assert_eq!(runs.load(Ordering::Relaxed), 1);
        // Per build key: another build, or the other wave mode, tries again.
        assert_eq!(cached_with(&cache, &[8; 32], false, b"bad", fail, true), reason);
        assert_eq!(cached_with(&cache, &[7; 32], true, b"bad", fail, true), reason);
        assert_eq!(runs.load(Ordering::Relaxed), 3);

        // With failures not kept, the cached failure is ignored and the
        // shader translated again on each call.
        let again = cached_with(&cache, &[7; 32], false, b"bad", fail, false).unwrap_err();
        assert!(again.ends_with("unsupported opcode v_foo"), "{again}");
        let again = cached_with(&cache, &[7; 32], false, b"bad", fail, false).unwrap_err();
        assert!(again.ends_with("unsupported opcode v_foo"), "{again}");
        assert_eq!(runs.load(Ordering::Relaxed), 5);

        // A build whose translator succeeds has an entry of its own.
        let ok = |_: &[u8]| Ok(shader());
        assert_eq!(cached_with(&cache, &[9; 32], false, b"bad", ok, true), Ok(shader()));
        assert_eq!(cached_with(&cache, &[9; 32], false, b"bad", fail, true), Ok(shader()));
        assert_eq!(runs.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn failure_entries_round_trip_and_reject_damage() {
        let bytes = encode_failure("no DXIL part");
        assert_eq!(read_entry(&bytes), Some(Err("no DXIL part".to_owned())));
        assert_eq!(read_entry(&encode(&shader())), Some(Ok(shader())));
        assert_eq!(read_entry(&[FAILED.as_slice(), &[0xff, 0xfe]].concat()), None);
        assert_eq!(read_entry(b"VSF0"), None);
    }

    #[test]
    fn reopening_the_cache_preserves_each_host_wave_translation() {
        let dir = tempfile::tempdir().unwrap();
        let input = b"synthetic wave reduction";
        let runs = AtomicU64::new(0);
        for wave32 in [false, true, false, true] {
            let cache = Cache::open(dir.path()).unwrap();
            let mut expected = shader();
            expected.stage = HwStage::Cs;
            expected.dxil.push(if wave32 { 32 } else { 64 });
            let result = cached(&cache, &[7; 32], wave32, input, |container| {
                assert_eq!(container, input);
                runs.fetch_add(1, Ordering::Relaxed);
                Ok(expected.clone())
            });
            assert_eq!(result, Ok(expected));
        }
        assert_eq!(runs.load(Ordering::Relaxed), 2);
    }

    fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            if e.path().is_dir() {
                files.extend(walk(&e.path()));
            } else {
                files.push(e.path());
            }
        }
        files
    }

    #[test]
    fn entries_round_trip_and_reject_damage() {
        let bytes = encode(&shader());
        assert_eq!(decode(&bytes), Some(shader()));
        assert_eq!(decode(&bytes[..bytes.len() - 1]), None);
        assert_eq!(decode(&[bytes.as_slice(), &[0]].concat()), None);
        assert_eq!(decode(b"VSH0"), None);
    }
}
