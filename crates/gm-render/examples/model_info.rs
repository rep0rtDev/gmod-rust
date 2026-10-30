//! Print what a model's files contain: GMOD_DIR=... cargo run -p gm-render --example model_info -- models/x.mdl ...
fn main() {
    let install = gm_content::find_install().unwrap();
    let mount = gm_content::Mount::for_install(&install).unwrap();
    for path in std::env::args().skip(1) {
        let Ok(bytes) = mount.read(&path) else {
            println!("{path}: missing");
            continue;
        };
        match gm_render::props::load_model(&mount, &path) {
            Err(e) => {
                println!("{path}: no mesh ({e})");
                let Ok(mdl) = gm_formats::mdl::Mdl::parse(&bytes) else {
                    continue;
                };
                let h = &mdl.header;
                let seqs = gm_formats::mdl::sequences(&bytes, h);
                let anims = gm_formats::mdl::anim_descs(&bytes, h);
                let (ani, blocks) = gm_formats::mdl::anim_blocks(&bytes);
                println!(
                    "  bones {} ani {ani:?} blocks {} includes {:?}",
                    mdl.bones.len(),
                    blocks.len(),
                    gm_formats::mdl::include_models(&bytes, h)
                );
                println!(
                    "  ani bytes {:?} first blocks {:?}",
                    mount.read(&ani).map(|b| b.len()).ok(),
                    &blocks[..blocks.len().min(3)]
                );
                println!(
                    "  pose params {:?}",
                    gm_formats::mdl::pose_params(&bytes, h)
                        .iter()
                        .map(|p| (p.name.as_str(), p.start, p.end, p.loop_range))
                        .collect::<Vec<_>>()
                );
                for s in &seqs {
                    let a = anims.get(s.anim.max(0) as usize);
                    println!(
                        "  seq {:?} anim {} -> {:?} layers {}",
                        s.name,
                        s.anim,
                        a.map(|a| (a.name.as_str(), a.fps, a.frames, a.flags, a.block)),
                        s.autolayers.len()
                    );
                    if s.group_size != [1, 1] && std::env::var_os("GRID").is_some() {
                        println!(
                            "      grid {:?} params {:?} {:?}..{:?} anims {:?}",
                            s.group_size,
                            s.param,
                            s.param_start,
                            s.param_end,
                            s.blends
                                .iter()
                                .map(|&b| anims
                                    .get(b as usize)
                                    .map(|a| a.name.as_str())
                                    .unwrap_or("?"))
                                .collect::<Vec<_>>()
                        );
                    }
                    if std::env::var_os("LAYERS").is_some() {
                        for l in &s.autolayers {
                            println!(
                                "      layer seq {} flags {:#x} {}..{}..{}..{}",
                                l.sequence, l.flags, l.start, l.peak, l.tail, l.end
                            );
                        }
                    }
                }
                println!("  {} seqs, {} anims", seqs.len(), anims.len());
            }
            Ok((mdl, vvd, vtx)) => {
                let h = &mdl.header;
                let mut lo = [f32::MAX; 3];
                let mut hi = [f32::MIN; 3];
                for v in &vvd.vertices {
                    for i in 0..3 {
                        lo[i] = lo[i].min(v.position[i]);
                        hi[i] = hi[i].max(v.position[i]);
                    }
                }
                println!(
                    "{path}: bones {} textures {:?} verts {} bbox {lo:?}..{hi:?} hull {:?}..{:?} eye {:?} includes {} bodyparts {} vtx bodyparts {}",
                    mdl.bones.len(),
                    mdl.textures.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
                    vvd.vertices.len(),
                    h.hull_min, h.hull_max, h.eye_position,
                    h.include_model_count, mdl.body_parts.len(), vtx.body_parts.len()
                );
                let mut materials: gm_render::props::MaterialMap = Default::default();
                let mut missing = Vec::new();
                let keys: Vec<Option<String>> = (0..mdl.textures.len())
                    .map(|t| {
                        gm_render::props::resolve_model_texture(
                            &mount,
                            &mdl,
                            t,
                            &mut materials,
                            &mut missing,
                        )
                    })
                    .collect();
                println!("  materials {keys:?} missing {missing:?}");
                let seqs = gm_formats::mdl::sequences(&bytes, h);
                let anims = gm_formats::mdl::anim_descs(&bytes, h);
                let (ani, blocks) = gm_formats::mdl::anim_blocks(&bytes);
                println!("  ani {ani:?} blocks {}", blocks.len());
                println!(
                    "  includes {:?}",
                    gm_formats::mdl::include_models(&bytes, h)
                );
                for s in seqs.iter().take(400) {
                    let a = anims.get(s.anim.max(0) as usize);
                    println!(
                        "  seq {:?} anim {} -> {:?}",
                        s.name,
                        s.anim,
                        a.map(|a| (a.name.as_str(), a.fps, a.frames, a.flags, a.block))
                    );
                    for l in &s.autolayers {
                        println!(
                            "      layer seq {} flags {:#x} {}..{}..{}..{}",
                            l.sequence, l.flags, l.start, l.peak, l.tail, l.end
                        );
                    }
                }
                println!("  {} seqs, {} anims", seqs.len(), anims.len());
                if std::env::var_os("BONES").is_some() {
                    for (i, b) in mdl.bones.iter().enumerate() {
                        println!("  bone {i} {} parent {}", b.name, b.parent);
                    }
                }
            }
        }
    }
}
