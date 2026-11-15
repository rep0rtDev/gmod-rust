//! GPU time per frame stage from timestamp queries (`cl_showfps 2`'s `gpu:` line).
//!
//! The CPU stage timers cannot see a GPU-bound frame: it shows up only as `acquire` (waiting for
//! the swapchain to hand back an image the GPU is still drawing). Stamps go into every encoder of
//! the frame in queue order, are resolved once at the end and read back a frame later.

use std::cell::{Cell, RefCell};
use std::sync::mpsc;

use crate::gpu::Gpu;

const MAX_STAMPS: u32 = 16;

pub struct GpuTimer {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    /// Nanoseconds per timestamp tick.
    period: f32,
    count: Cell<u32>,
    labels: RefCell<Vec<&'static str>>,
    /// A resolved frame is on its way back (no new stamps until it is read).
    pending: Cell<bool>,
    rx: RefCell<Option<mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>>>,
    /// Smoothed milliseconds per stage (label of the stamp that ends it) and the total.
    pub last: RefCell<Vec<(&'static str, f32)>>,
}

impl GpuTimer {
    /// None when the adapter has no timestamp queries inside encoders (Metal, some GLs).
    pub fn new(gpu: &Gpu) -> Option<Self> {
        let need =
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        if !gpu.device.features().contains(need) {
            return None;
        }
        let set = gpu.device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("frame timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: MAX_STAMPS,
        });
        let size = MAX_STAMPS as u64 * 8;
        let resolve = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("timestamps resolve"),
            size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("timestamps read"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Some(Self {
            set,
            resolve,
            read,
            period: gpu.queue.get_timestamp_period(),
            count: Cell::new(0),
            labels: RefCell::new(Vec::new()),
            pending: Cell::new(false),
            rx: RefCell::new(None),
            last: RefCell::new(Vec::new()),
        })
    }

    /// Timestamp now (in queue order); `label` names the stage this stamp ends.
    pub fn stamp(&self, enc: &mut wgpu::CommandEncoder, label: &'static str) {
        let n = self.count.get();
        if self.pending.get() || n >= MAX_STAMPS {
            return;
        }
        enc.write_timestamp(&self.set, n);
        self.labels.borrow_mut().push(label);
        self.count.set(n + 1);
    }

    /// Resolve the frame's stamps into the readback buffer (before the last submit).
    pub fn end_frame(&self, enc: &mut wgpu::CommandEncoder) {
        let n = self.count.get();
        if self.pending.get() {
            return;
        }
        if n < 2 {
            self.count.set(0);
            self.labels.borrow_mut().clear();
            return;
        }
        enc.resolve_query_set(&self.set, 0..n, &self.resolve, 0);
        enc.copy_buffer_to_buffer(&self.resolve, 0, &self.read, 0, n as u64 * 8);
        self.pending.set(true);
    }

    /// Pick up the readback when it has landed (call once a frame, after the submit).
    pub fn collect(&self, gpu: &Gpu) {
        if !self.pending.get() {
            return;
        }
        let n = self.count.get() as u64;
        let mut rx = self.rx.borrow_mut();
        match rx.as_ref() {
            None => {
                let (tx, r) = mpsc::channel();
                self.read
                    .slice(0..n * 8)
                    .map_async(wgpu::MapMode::Read, move |res| {
                        let _ = tx.send(res);
                    });
                *rx = Some(r);
            }
            Some(r) => {
                let _ = gpu.device.poll(wgpu::PollType::Poll);
                let Ok(res) = r.try_recv() else {
                    return;
                };
                let ticks: Vec<u64> = match self.read.slice(0..n * 8).get_mapped_range() {
                    Ok(data) if res.is_ok() => data
                        .as_chunks::<8>()
                        .0
                        .iter()
                        .map(|c| u64::from_le_bytes(*c))
                        .collect(),
                    _ => Vec::new(),
                };
                if ticks.len() >= 2 {
                    let labels = self.labels.borrow();
                    let ms = |a: u64, b: u64| b.saturating_sub(a) as f32 * self.period / 1.0e6;
                    let mut now: Vec<(&'static str, f32)> = (1..ticks.len())
                        .map(|i| (labels[i], ms(ticks[i - 1], ticks[i])))
                        .collect();
                    now.push(("total", ms(ticks[0], ticks[ticks.len() - 1])));
                    let mut last = self.last.borrow_mut();
                    if last.len() == now.len() && last.iter().zip(&now).all(|(a, b)| a.0 == b.0) {
                        for (l, (_, v)) in last.iter_mut().zip(&now) {
                            l.1 = l.1 * 0.9 + v * 0.1;
                        }
                    } else {
                        *last = now;
                    }
                }
                self.read.unmap();
                *rx = None;
                self.pending.set(false);
                self.count.set(0);
                self.labels.borrow_mut().clear();
            }
        }
    }

    /// `gpu: shadow 0.41 reflect 1.20 ... total 4.9` for the fps panel.
    pub fn line(&self) -> String {
        let last = self.last.borrow();
        if last.is_empty() {
            return "gpu: (no timestamps yet)".to_owned();
        }
        let mut s = String::from("gpu:");
        for (l, v) in last.iter() {
            s.push_str(&format!(" {l} {v:.2}"));
        }
        s
    }
}
