use decktector_core::sim::{build_sim_track, SIM_TRACKS};

#[test]
fn probe() {
    let spec = &SIM_TRACKS[0];
    let t = build_sim_track(spec, 1);
    let bar_ms = 60_000.0 / spec.bpm * 4.0;
    let wf = t.waveform.as_ref().unwrap();
    let detail = wf.detail.as_ref().unwrap();
    let ms_per = wf.ms_per_detail_segment;
    let bars = (t.duration_ms / bar_ms).floor() as usize;
    println!(
        "bpm {} bar {:.1}ms bars {} detail {} ms_per {:.3}",
        spec.bpm,
        bar_ms,
        bars,
        detail.len(),
        ms_per
    );

    // mean height per bar, from the same data the analysis reads
    let mut energies = Vec::new();
    for b in 0..bars {
        let s = (b as f64 * bar_ms / ms_per).floor() as usize;
        let e = (((b + 1) as f64 * bar_ms / ms_per).ceil() as usize).min(detail.len());
        if s >= e {
            break;
        }
        let m: f32 = detail[s..e].iter().map(|c| c.height).sum::<f32>() / (e - s) as f32;
        energies.push(m);
    }
    let mut sorted = energies.clone();
    sorted.sort_by(f32::total_cmp);
    let median = sorted[sorted.len() / 2];
    println!("median bar energy {median:.3}");
    print!("bar energies (x100):");
    for (i, en) in energies.iter().enumerate() {
        if i % 16 == 0 {
            print!("\n  bar {i:>3} (phrase {:>2}): ", i / 16);
        }
        print!("{:>3} ", (en * 100.0).round() as i32);
    }
    println!();
    println!("\nevents:");
    for e in &t.events {
        println!(
            "  {:?} bars {:.0}-{:.0}  ({:.1}s-{:.1}s)  conf {:.2}",
            e.kind,
            e.start_ms / bar_ms,
            e.end_ms / bar_ms,
            e.start_ms / 1000.0,
            e.end_ms / 1000.0,
            e.confidence
        );
    }
}
