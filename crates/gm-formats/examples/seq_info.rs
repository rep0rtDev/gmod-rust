//! cargo run -p gm-formats --example seq_info -- <dir.vpk> <model.mdl> [name-filter]: every
//! sequence of a model and its include models with the animation's fps / frames / period /
//! loop flag / ground speed, and each blend animation's ground speed.
fn main() {
    let mut args = std::env::args().skip(1);
    let vpk = gm_formats::vpk::Archive::open(args.next().unwrap()).unwrap();
    let path = args.next().unwrap();
    let filter = args.next().unwrap_or_default().to_ascii_lowercase();
    let bytes = vpk.read(&path).unwrap();
    let mdl = gm_formats::mdl::Mdl::parse(&bytes).unwrap();
    let mut files = vec![(path.clone(), bytes.clone())];
    for inc in gm_formats::mdl::include_models(&bytes, &mdl.header) {
        if let Ok(b) = vpk.read(&inc) {
            files.push((inc, b));
        }
    }
    for (file, data) in &files {
        let Ok(m) = gm_formats::mdl::Mdl::parse(data) else {
            continue;
        };
        let seqs = gm_formats::mdl::sequences(data, &m.header);
        let anims = gm_formats::mdl::anim_descs(data, &m.header);
        for q in &seqs {
            if !filter.is_empty() && !q.name.to_ascii_lowercase().contains(&filter) {
                continue;
            }
            let a = anims.get(usize::try_from(q.anim).unwrap_or(usize::MAX));
            let (fps, frames, flags, speed) = a.map_or((0.0, 0, 0, 0.0), |a| {
                (a.fps, a.frames, a.flags, a.ground_speed())
            });
            let period = if fps > 0.0 {
                (frames - 1).max(1) as f32 / fps
            } else {
                0.0
            };
            println!(
                "{file} {} act={} w={} fps={fps} frames={frames} period={period:.4}s loop={} speed={speed:.1}",
                q.name,
                q.activity_name,
                q.act_weight,
                flags & 1 != 0
            );
            if q.blends.len() > 1 {
                let row: Vec<String> = q
                    .blends
                    .iter()
                    .map(|&b| {
                        anims
                            .get(usize::try_from(b).unwrap_or(usize::MAX))
                            .map_or_else(
                                || format!("{b}:?"),
                                |a| format!("{}={:.1}", a.name, a.ground_speed()),
                            )
                    })
                    .collect();
                println!(
                    "    blend {:?} param {:?} {:?}..{:?}: {}",
                    q.group_size,
                    q.param,
                    q.param_start,
                    q.param_end,
                    row.join(" ")
                );
            }
        }
    }
}
