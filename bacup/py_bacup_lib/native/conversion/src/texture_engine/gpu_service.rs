//! GpuService — the texture engine's single GPU submission thread. One
//! dedicated thread calls the BC7 batch FFI, drain-coalescing queued chains
//! into larger dispatch batches. Rayon workers submit whole mip chains and
//! block on a reply. Small textures (< min_pixels) and queue overflow encode
//! on the calling worker — the worker pool IS the CPU BC7 pool. start_cpu_only()
//! (--cpu-textures) spawns no thread; everything CPU-encodes.
//!
//! The process-global device mutex in directxtex remains underneath; since
//! this thread is the engine's only submitter, the mutex is uncontended.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crossbeam_channel::{Receiver, Sender, bounded};

/// Max pixels coalesced into one GPU dispatch batch.
const MAX_COALESCED_PIXELS: u64 = 32 * 1024 * 1024;

struct GpuRequest {
    images: Vec<(u32, u32, Vec<u8>)>,
    srgb: bool,
    reply: Sender<GpuReply>,
}

enum GpuReply {
    Payloads(Vec<Vec<u8>>),
    /// GPU failed/unavailable — images returned so the caller CPU-encodes.
    Bounce(Vec<(u32, u32, Vec<u8>)>),
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GpuServiceStats {
    pub gpu_submissions: u64,
    pub gpu_dispatch_batches: u64,
    pub cpu_encodes: u64,
    pub overflow_to_cpu: u64,
    pub gpu_failures: u64,
}

#[derive(Default)]
struct Counters {
    gpu_submissions: AtomicU64,
    gpu_dispatch_batches: AtomicU64,
    cpu_encodes: AtomicU64,
    overflow_to_cpu: AtomicU64,
    gpu_failures: AtomicU64,
}

pub struct GpuService {
    tx: Option<Sender<GpuRequest>>,
    handle: Option<std::thread::JoinHandle<()>>,
    counters: Arc<Counters>,
}

fn cpu_encode_chain(images: &[(u32, u32, Vec<u8>)], srgb: bool) -> Result<Vec<Vec<u8>>, String> {
    let format = if srgb { "BC7_UNORM_SRGB" } else { "BC7_UNORM" };
    images
        .iter()
        .map(|(w, h, px)| {
            // Single-level chain through the legacy CPU path, header stripped
            // (DX10 header = 148 bytes). Same encoder + BC7_QUICK flags as the
            // legacy per-mip fallback.
            let bytes = directxtex_native::encode_dds_from_rgba8_chain(
                &[(*w, *h, px.clone())],
                format,
                false,
                None,
            )?;
            bytes
                .get(148..)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| "BC7 CPU encode produced no payload".to_string())
        })
        .collect()
}

impl GpuService {
    pub fn start(queue_cap: usize) -> Self {
        let (tx, rx): (Sender<GpuRequest>, Receiver<GpuRequest>) = bounded(queue_cap.max(1));
        let counters = Arc::new(Counters::default());
        let thread_counters = Arc::clone(&counters);
        let handle = std::thread::Builder::new()
            .name("texture-engine-gpu".to_string())
            .spawn(move || submission_loop(rx, thread_counters))
            .expect("spawn gpu service thread");
        Self {
            tx: Some(tx),
            handle: Some(handle),
            counters,
        }
    }

    /// --cpu-textures: no thread, every encode on the calling worker.
    pub fn start_cpu_only() -> Self {
        Self {
            tx: None,
            handle: None,
            counters: Arc::new(Counters::default()),
        }
    }

    /// Encode one texture's mip chain to BC7 payloads (no DDS headers).
    /// Never fails on GPU trouble — the CPU fallback is always taken.
    pub fn encode_bc7(
        &self,
        images: Vec<(u32, u32, Vec<u8>)>,
        srgb: bool,
        min_pixels: u32,
    ) -> Result<Vec<Vec<u8>>, String> {
        if images.is_empty() {
            return Ok(Vec::new());
        }
        let base_pixels = u64::from(images[0].0) * u64::from(images[0].1);
        if self.tx.is_none() || base_pixels < u64::from(min_pixels) {
            self.counters.cpu_encodes.fetch_add(1, Ordering::Relaxed);
            return cpu_encode_chain(&images, srgb);
        }
        let (reply_tx, reply_rx) = bounded(1);
        let request = GpuRequest {
            images,
            srgb,
            reply: reply_tx,
        };
        match self.tx.as_ref().expect("checked above").try_send(request) {
            Ok(()) => match reply_rx.recv() {
                Ok(GpuReply::Payloads(payloads)) => {
                    self.counters
                        .gpu_submissions
                        .fetch_add(1, Ordering::Relaxed);
                    Ok(payloads)
                }
                Ok(GpuReply::Bounce(images)) => {
                    self.counters.gpu_failures.fetch_add(1, Ordering::Relaxed);
                    self.counters.cpu_encodes.fetch_add(1, Ordering::Relaxed);
                    cpu_encode_chain(&images, srgb)
                }
                Err(_) => Err("gpu service thread terminated".to_string()),
            },
            Err(crossbeam_channel::TrySendError::Full(request)) => {
                self.counters
                    .overflow_to_cpu
                    .fetch_add(1, Ordering::Relaxed);
                self.counters.cpu_encodes.fetch_add(1, Ordering::Relaxed);
                let started = std::time::Instant::now();
                let result = cpu_encode_chain(&request.images, srgb);
                let elapsed_ms = started.elapsed().as_millis();
                if elapsed_ms > 2000 {
                    eprintln!(
                        "[gpu_timing] slow CPU overflow encode pixels={} elapsed_ms={elapsed_ms}",
                        chain_pixels(&request.images)
                    );
                }
                result
            }
            Err(crossbeam_channel::TrySendError::Disconnected(request)) => {
                self.counters.cpu_encodes.fetch_add(1, Ordering::Relaxed);
                cpu_encode_chain(&request.images, srgb)
            }
        }
    }

    pub fn stats(&self) -> GpuServiceStats {
        GpuServiceStats {
            gpu_submissions: self.counters.gpu_submissions.load(Ordering::Relaxed),
            gpu_dispatch_batches: self.counters.gpu_dispatch_batches.load(Ordering::Relaxed),
            cpu_encodes: self.counters.cpu_encodes.load(Ordering::Relaxed),
            overflow_to_cpu: self.counters.overflow_to_cpu.load(Ordering::Relaxed),
            gpu_failures: self.counters.gpu_failures.load(Ordering::Relaxed),
        }
    }

    pub fn shutdown(mut self) {
        self.tx.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for GpuService {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn submission_loop(rx: Receiver<GpuRequest>, counters: Arc<Counters>) {
    let mut gpu_dead = false;
    while let Ok(first) = rx.recv() {
        let mut batch = vec![first];
        let mut pixels: u64 = chain_pixels(&batch[0].images);
        loop {
            if pixels >= MAX_COALESCED_PIXELS {
                break;
            }
            match rx.try_recv() {
                Ok(req) if req.srgb == batch[0].srgb => {
                    pixels += chain_pixels(&req.images);
                    batch.push(req);
                }
                Ok(req) => {
                    process_batch(
                        std::mem::replace(&mut batch, vec![req]),
                        &mut gpu_dead,
                        &counters,
                    );
                    pixels = chain_pixels(&batch[0].images);
                }
                Err(_) => break,
            }
        }
        process_batch(batch, &mut gpu_dead, &counters);
    }
}

fn chain_pixels(images: &[(u32, u32, Vec<u8>)]) -> u64 {
    images
        .iter()
        .map(|(w, h, _)| u64::from(*w) * u64::from(*h))
        .sum()
}

fn process_batch(batch: Vec<GpuRequest>, gpu_dead: &mut bool, counters: &Counters) {
    if batch.is_empty() {
        return;
    }
    if *gpu_dead {
        for req in batch {
            let _ = req.reply.send(GpuReply::Bounce(req.images));
        }
        return;
    }
    let srgb = batch[0].srgb;
    let refs: Vec<(u32, u32, &[u8])> = batch
        .iter()
        .flat_map(|r| r.images.iter().map(|(w, h, p)| (*w, *h, p.as_slice())))
        .collect();
    let pixels: u64 = refs
        .iter()
        .map(|(w, h, _)| u64::from(*w) * u64::from(*h))
        .sum();
    let started = std::time::Instant::now();
    match directxtex_native::compress_bc7_gpu_batch(&refs, srgb) {
        Ok(mut payloads) => {
            counters
                .gpu_dispatch_batches
                .fetch_add(1, Ordering::Relaxed);
            let elapsed_ms = started.elapsed().as_millis();
            if elapsed_ms > 1000 {
                eprintln!(
                    "[gpu_timing] slow dispatch chains={} pixels={pixels} elapsed_ms={elapsed_ms}",
                    batch.len()
                );
            }
            for req in batch {
                let rest = payloads.split_off(req.images.len());
                let mine = std::mem::replace(&mut payloads, rest);
                let _ = req.reply.send(GpuReply::Payloads(mine));
            }
        }
        Err(err) => {
            // Device unavailable or dispatch failed — mark dead (later
            // requests bounce fast) and bounce this batch to the callers' CPU.
            // This must be LOUD: from here on every large texture silently
            // CPU-encodes, which looks like the wave "randomly stalling".
            *gpu_dead = true;
            eprintln!(
                "[gpu_timing] GPU DEAD after {}ms — all remaining large textures fall back to CPU BC7: {err}",
                started.elapsed().as_millis()
            );
            for req in batch {
                let _ = req.reply.send(GpuReply::Bounce(req.images));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(seed: u8) -> Vec<(u32, u32, Vec<u8>)> {
        [(8u32, 8u32), (4, 4), (2, 2), (1, 1)]
            .iter()
            .map(|(w, h)| {
                let px = (0..(*w as usize) * (*h as usize))
                    .flat_map(|i| [(i as u8).wrapping_add(seed), 7, 99, 255])
                    .collect();
                (*w, *h, px)
            })
            .collect()
    }

    #[test]
    fn cpu_only_service_encodes_without_gpu() {
        let svc = GpuService::start_cpu_only();
        let out = svc.encode_bc7(chain(1), false, 512 * 512).unwrap();
        assert_eq!(out.len(), 4);
        assert_eq!(out[0].len(), 4 * 16); // 8x8 BC7 = 4 blocks
        assert_eq!(svc.stats().cpu_encodes, 1);
        assert_eq!(svc.stats().gpu_submissions, 0);
        svc.shutdown();
    }

    #[test]
    fn small_textures_stay_on_cpu_even_with_gpu_service() {
        let svc = GpuService::start(4);
        // 8x8 base < 512^2 cutoff -> CPU on the calling thread.
        let out = svc.encode_bc7(chain(2), false, 512 * 512).unwrap();
        assert_eq!(out.len(), 4);
        assert_eq!(svc.stats().gpu_submissions, 0);
        svc.shutdown();
    }

    #[test]
    fn service_output_matches_direct_batch_when_gpu_present() {
        let imgs = chain(3);
        let refs: Vec<(u32, u32, &[u8])> = imgs
            .iter()
            .map(|(w, h, p)| (*w, *h, p.as_slice()))
            .collect();
        let direct = match directxtex_native::compress_bc7_gpu_batch(&refs, false) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("skip: no usable GPU ({e})");
                return;
            }
        };
        let svc = GpuService::start(4);
        let via_service = svc.encode_bc7(imgs, false, 0).unwrap(); // min_pixels 0 -> GPU path
        assert_eq!(via_service, direct);
        assert_eq!(svc.stats().gpu_submissions, 1);
        svc.shutdown();
    }

    fn bench_chain(base: u32, seed: u8) -> Vec<(u32, u32, Vec<u8>)> {
        let mut chain = Vec::new();
        let mut size = base;
        while size >= 4 {
            let px: Vec<u8> = (0..(size as usize) * (size as usize))
                .flat_map(|i| {
                    [
                        (i as u8).wrapping_mul(31).wrapping_add(seed),
                        (i >> 3) as u8,
                        (i >> 7) as u8 ^ seed,
                        255,
                    ]
                })
                .collect();
            chain.push((size, size, px));
            size /= 2;
        }
        chain
    }

    /// Manual A/B: whole-chain-to-GPU (old policy, min_pixels=0 sends every
    /// level) vs per-level split (new policy). Run explicitly:
    /// cargo test -p conversion_native bench_split_policy -- --ignored --nocapture
    #[test]
    #[ignore = "manual GPU speed comparison"]
    fn bench_split_policy_vs_whole_chain() {
        let probe = GpuService::start(1);
        let gpu_works = probe.encode_bc7(bench_chain(8, 1), false, 0).is_ok()
            && probe.stats().gpu_submissions == 1;
        probe.shutdown();
        if !gpu_works {
            eprintln!("[bench] no usable GPU; skipping");
            return;
        }

        let workers = 16usize;
        let bases: [u32; 8] = [2048, 1024, 1024, 512, 512, 512, 512, 512];
        // Pre-generate every chain OUTSIDE the timed section; both arms encode
        // clones of the same inputs.
        let inputs: Vec<Vec<Vec<(u32, u32, Vec<u8>)>>> = (0..workers)
            .map(|w| {
                bases
                    .iter()
                    .enumerate()
                    .map(|(t, base)| bench_chain(*base, (w * 8 + t) as u8))
                    .collect()
            })
            .collect();
        for (label, use_gpu) in [("gpu(production)", true), ("cpu-only(16 workers)", false)] {
            let svc = if use_gpu {
                GpuService::start(8)
            } else {
                GpuService::start_cpu_only()
            };
            let min_pixels = 512u32 * 512;
            let started = std::time::Instant::now();
            std::thread::scope(|scope| {
                for worker_chains in &inputs {
                    let svc = &svc;
                    scope.spawn(move || {
                        for chain in worker_chains {
                            svc.encode_bc7(chain.clone(), false, min_pixels).unwrap();
                        }
                    });
                }
            });
            let elapsed = started.elapsed();
            let total = workers * bases.len();
            eprintln!(
                "[bench] {label}: {total} textures in {:.2}s ({:.1} tex/s) stats={:?}",
                elapsed.as_secs_f64(),
                total as f64 / elapsed.as_secs_f64(),
                svc.stats()
            );
            svc.shutdown();
        }
    }

    #[test]
    fn gpu_path_always_returns_payloads_via_bounce_fallback() {
        // On a GPU-less box the service bounces and the caller CPU-encodes;
        // on a GPU box it encodes on the device. Either way: 4 valid payloads.
        let svc = GpuService::start(2);
        let out = svc.encode_bc7(chain(4), true, 0).unwrap();
        assert_eq!(out.len(), 4);
        assert_eq!(out[0].len(), 4 * 16);
        svc.shutdown();
    }
}
