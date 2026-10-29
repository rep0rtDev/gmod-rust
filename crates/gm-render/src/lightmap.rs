//! Shelf-packed lightmap atlas. Each face lightmap (styles[0] only for now)
//! becomes a rectangle; the texture stores linear light divided by two.

use gm_formats::bsp::lumps::ColorRgbExp32;

pub struct Atlas {
    pub size: u32,
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
            pixels: vec![0; (size * size * 4) as usize],
            shelf_y: 0,
            shelf_h: 0,
            cursor_x: 0,
        }
    }

    /// Place a `w*h` block with a one-pixel gutter. Returns None when full.
    pub fn place(&mut self, w: u32, h: u32, samples: &[ColorRgbExp32]) -> Option<Placement> {
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
        for gy in 0..ph {
            for gx in 0..pw {
                let sx = (gx.max(1) - 1).min(w - 1);
                let sy = (gy.max(1) - 1).min(h - 1);
                let c = samples[(sy * w + sx) as usize].to_linear();
                let i = (((p.y - 1 + gy) * self.size + (p.x - 1 + gx)) * 4) as usize;
                self.pixels[i] = (c[0] * 0.5 * 255.0).clamp(0.0, 255.0) as u8;
                self.pixels[i + 1] = (c[1] * 0.5 * 255.0).clamp(0.0, 255.0) as u8;
                self.pixels[i + 2] = (c[2] * 0.5 * 255.0).clamp(0.0, 255.0) as u8;
                self.pixels[i + 3] = 255;
            }
        }
        Some(p)
    }

    /// One full-bright luxel (linear 1.0) for faces without lighting.
    pub fn white(&mut self) -> Option<Placement> {
        self.place(
            1,
            1,
            &[ColorRgbExp32 {
                r: 255,
                g: 255,
                b: 255,
                exponent: 0,
            }],
        )
    }

    /// Texture coordinate of the centre of a placement's luxel (x, y).
    pub fn luxel_center(&self, p: &Placement, x: f32, y: f32) -> [f32; 2] {
        [
            (p.x as f32 + x + 0.5) / self.size as f32,
            (p.y as f32 + y + 0.5) / self.size as f32,
        ]
    }
}
