//! Times the driver's compile of a mesh + pixel pipeline from two console
//! containers, as the title's run makes it: each stage translated alone,
//! and both translated with the mesh shader's link (#283) when it has one.
//! `cargo run --release --features host --example mesh_time -- <ms.cdxil> <ps.cdxil> [out dir]`
//!
//! Set `VICTORY_ROOT_NONCE=<random>` for each run: the driver's disk cache
//! otherwise answers a pipeline it has compiled before. The out dir, when
//! given, receives the translated DXIL (`ms.dxil`, `ps.dxil`, `ms.linked.dxil`,
//! `ps.linked.dxil`).

// The Agility SDK exports `build.rs` links into this crate's executables.
#[cfg(windows)]
#[path = "../src/agility.rs"]
mod agility;

#[cfg(all(windows, feature = "host"))]
fn main() {
    use victory_shader::emit;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [ms, ps, rest @ ..] = &args[..] else {
        eprintln!("usage: mesh_time <ms.cdxil> <ps.cdxil> [out dir]");
        std::process::exit(2);
    };
    let read = |p: &str| std::fs::read(p).unwrap_or_else(|e| panic!("read {p}: {e}"));
    let (ms, ps) = (read(ms), read(ps));
    let out = rest.first().map(std::path::PathBuf::from);
    let host = emit::host::Host::new(false).expect("a D3D12 device");
    if let Some(n) = host.max_wave_lanes() {
        emit::set_host_max_wave(n);
    }
    let time = |name: &str, ms: &emit::HostShader, ps: &emit::HostShader| {
        if let Some(dir) = &out {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(dir.join(format!("ms{name}.dxil")), &ms.dxil);
            let _ = std::fs::write(dir.join(format!("ps{name}.dxil")), &ps.dxil);
        }
        let started = std::time::Instant::now();
        let made = host.mesh_pipeline(ms, ps).map(drop);
        println!(
            "{:<8} {:>7} ms  (ms {} bytes, ps {} bytes) {:?}",
            if name.is_empty() { "plain" } else { &name[1..] },
            started.elapsed().as_millis(),
            ms.dxil.len(),
            ps.dxil.len(),
            made.err()
        );
    };
    let started = std::time::Instant::now();
    let (tm, tp) = (emit::translate(&ms).expect("mesh translates"), emit::translate(&ps).expect("pixel translates"));
    println!("translate {} ms", started.elapsed().as_millis());
    // `MESH_TIME_THREADS=<n>`: n threads compile the plain pair at once on
    // this one device (the title's workers share theirs), each with its own
    // root signature so that none answers for another.
    if let Some(n) = std::env::var("MESH_TIME_THREADS").ok().and_then(|n| n.parse::<u32>().ok()) {
        let started = std::time::Instant::now();
        std::thread::scope(|s| {
            for i in 0..n {
                let (host, tm, tp) = (&host, &tm, &tp);
                s.spawn(move || {
                    let mut ms = tm.clone();
                    // A larger root constant (by 4 dwords a thread): another root signature.
                    let mut last = ms.user_data.last().cloned().expect("user data");
                    last.index += 4 * (i + 1);
                    ms.user_data.push(last);
                    let t = std::time::Instant::now();
                    let made = host.mesh_pipeline(&ms, tp).map(drop);
                    println!("thread {i}: {} ms {:?}", t.elapsed().as_millis(), made.err());
                });
            }
        });
        println!("{n} threads: {} ms in all", started.elapsed().as_millis());
        return;
    }
    let link = emit::mesh_link(&ms);
    if std::env::var_os("MESH_TIME_ONLY_LINKED").is_none() || link.is_none() {
        time("", &tm, &tp);
    }
    if let Some(link) = link {
        let lm = emit::translate_linked(&ms, &link).expect("mesh translates linked");
        let lp = emit::translate_linked(&ps, &link).expect("pixel translates linked");
        time(".linked", &lm, &lp);
    }
}

#[cfg(not(all(windows, feature = "host")))]
fn main() {
    eprintln!("mesh_time needs Windows and the `host` feature");
}
