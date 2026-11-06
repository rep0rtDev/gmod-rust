//! Shelf-packed lightmap atlas. Each face lightmap (its light styles summed at their
//! brightness) becomes a rectangle; the texture stores linear light.

use gm_formats::bsp::Bsp;

/// The engine's built-in light style patterns (`a` = 0, `m` = 1, `z` = 25/12); style 0 is
/// steady full brightness, 1..=12 animate and are taken at their mean here.
const STYLE_PATTERNS: [&str; 13] = [
    "m",
    "mmnmmommommnonmmonqnmmo",
    "abcdefghijklmnopqrstuvwxyzyxwvutsrqponmlkjihgfedcba",
    "mmmmmaaaaammmmmaaaaaabcdefgabcdefg",
    "mamamamamama",
    "jklmnopqrstuvwxyzyxwvutsrqponmlkj",
    "nmonqnmomnmomomno",
    "mmmaaaabcdefgmmmmaaaammmaamm",
    "mmmaaammmaaammmabcdefaaaammmmabcdefmmmaaaa",
    "aaaaaaaazzzzzzzz",
    "mmamammmmammamamaaamammma",
    "abcdefghijklmnopqrrqponmlkjihgfedcba",
    "mmnnmmnnnmmnn",
];

/// Mean brightness of a light style pattern string.
pub fn pattern_mean(pattern: &str) -> f32 {
    let n = pattern.len().max(1) as f32;
    pattern
        .bytes()
        .map(|c| (c.saturating_sub(b'a')) as f32 / 12.0)
        .sum::<f32>()
        / n
}

/// Brightness per light style (index 0..256): 0 steady, 1..=12 the built-in patterns at their
/// mean, 32.. the map's named lights (`light_environment` "sun_light" on gm_construct is one)
/// at full brightness unless the light spawns initially dark (`spawnflags` bit 0).
pub fn style_brightness(bsp: &Bsp<'_>) -> Vec<f32> {
    let mut v = vec![1.0f32; 256];
    for (i, p) in STYLE_PATTERNS.iter().enumerate() {
        v[i] = pattern_mean(p);
    }
    for e in &bsp.entities {
        let class = e.classname();
        if !(class == "light" || class == "light_spot" || class == "light_environment") {
            continue;
        }
        let Some(style) = e.get("style").and_then(|s| s.trim().parse::<usize>().ok()) else {
            continue;
        };
        let flags = e
            .get("spawnflags")
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(0);
        if (32..256).contains(&style) && flags & 1 != 0 && class != "light_environment" {
            v[style] = 0.0;
        }
    }
    v
}

/// A `w` x `h` block of linear luxels as Rgba16Float texels with the atlas's one-texel
/// clamp border: `(w + 2) * (h + 2)` texels, row-major.
pub fn encode_block(w: u32, h: u32, samples: &[[f32; 3]]) -> Vec<u8> {
    let (pw, ph) = (w + 2, h + 2);
    let mut out = vec![0u8; (pw * ph * 8) as usize];
    for gy in 0..ph {
        for gx in 0..pw {
            let sx = (gx.max(1) - 1).min(w - 1);
            let sy = (gy.max(1) - 1).min(h - 1);
            let c = samples[(sy * w + sx) as usize];
            let i = ((gy * pw + gx) * 8) as usize;
            for (k, v) in [c[0], c[1], c[2], 1.0].into_iter().enumerate() {
                let bytes = half::f16::from_f32(v.max(0.0)).to_le_bytes();
                out[i + k * 2..i + k * 2 + 2].copy_from_slice(&bytes);
            }
        }
    }
    out
}

pub struct Atlas {
    pub size: u32,
    /// Rgba16Float texels: linear light as stored in the BSP (HDR values pass through).
    pub pixels: Vec<u8>,
    shelf_y: u32,
    shelf_h: u32,
    cursor_x: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Atlas {
    pub fn new(size: u32) -> Self {
        Atlas {
            size,
            pixels: vec![0; (size * size * 8) as usize],
            shelf_y: 0,
            shelf_h: 0,
            cursor_x: 0,
        }
    }

    /// Place a `w*h` block of linear light with a one-pixel gutter. Returns None when full.
    pub fn place(&mut self, w: u32, h: u32, samples: &[[f32; 3]]) -> Option<Placement> {
        let pw = w + 2;
        let ph = h + 2;
        if pw > self.size || ph > self.size {
            return None;
        }
        if self.cursor_x + pw > self.size {
            self.shelf_y += self.shelf_h;
            self.shelf_h = 0;
            self.cursor_x = 0;
        }
        if self.shelf_y + ph > self.size {
            return None;
        }
        let p = Placement {
            x: self.cursor_x + 1,
            y: self.shelf_y + 1,
            w,
            h,
        };
        self.cursor_x += pw;
        self.shelf_h = self.shelf_h.max(ph);
        let block = encode_block(w, h, samples);
        let row = (pw * 8) as usize;
        for gy in 0..ph {
            let dst = (((p.y - 1 + gy) * self.size + (p.x - 1)) * 8) as usize;
            let src = gy as usize * row;
            self.pixels[dst..dst + row].copy_from_slice(&block[src..src + row]);
        }
        Some(p)
    }

    /// One full-bright luxel (linear 1.0) for faces without lighting.
    pub fn white(&mut self) -> Option<Placement> {
        self.place(1, 1, &[[1.0; 3]])
    }

    /// Texture coordinate of the centre of a placement's luxel (x, y).
    pub fn luxel_center(&self, p: &Placement, x: f32, y: f32) -> [f32; 2] {
        [
            (p.x as f32 + x + 0.5) / self.size as f32,
            (p.y as f32 + y + 0.5) / self.size as f32,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::pattern_mean;

    #[test]
    fn pattern_means() {
        assert_eq!(pattern_mean("m"), 1.0);
        assert_eq!(pattern_mean("a"), 0.0);
        assert!((pattern_mean("z") - 25.0 / 12.0).abs() < 1e-6);
        assert!((pattern_mean("mamamamamama") - 0.5).abs() < 1e-6);
    }
}
