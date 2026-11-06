//! List a model's sequences: `seq_list <file.mdl | dir.vpk:models/x.mdl> [substring...]` prints label, activity,
//! weight, fps, frames and seconds (sequence -> its first animation) for sequences whose
//! label or activity contains any of the substrings (all when none).
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data = match args[0].split_once(".vpk:") {
        Some((vpk, inner)) => gm_formats::vpk::Archive::open(format!("{vpk}.vpk"))
            .expect("vpk")
            .read(inner)
            .expect("mdl in vpk"),
        None => std::fs::read(&args[0]).expect("mdl"),
    };
    let header = gm_formats::mdl::Header::parse(&data).expect("header");
    let seqs = gm_formats::mdl::sequences(&data, &header);
    eprintln!(
        "{} sequences, includes {:?}",
        seqs.len(),
        gm_formats::mdl::include_models(&data, &header)
    );
    let anims = gm_formats::mdl::anim_descs(&data, &header);
    for s in &seqs {
        let hit = args.len() == 1
            || args[1..]
                .iter()
                .any(|f| s.name.contains(f.as_str()) || s.activity_name.contains(f.as_str()));
        if !hit {
            continue;
        }
        let a = anims.get(s.anim as usize);
        let (fps, frames) = a.map_or((0.0, 0), |a| (a.fps, a.frames));
        let secs = if fps > 0.0 {
            (frames.max(1) - 1) as f32 / fps
        } else {
            0.0
        };
        println!(
            "{:<24} {:<28} w{} fps {:>5.1} frames {:>3} {:>5.2}s ticks {:>3.0}",
            s.name,
            s.activity_name,
            s.act_weight,
            fps,
            frames,
            secs,
            secs * 66.6667
        );
    }
}
