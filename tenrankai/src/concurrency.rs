//! Worker-count policy for CPU-heavy image generation.

use std::sync::atomic::{AtomicUsize, Ordering};

pub const DEFAULT_INTERACTIVE_RATIO: f64 = 0.5;
pub const DEFAULT_BACKGROUND_RATIO: f64 = 0.25;
pub const DEFAULT_HARD_MAX_WORKERS: usize = 64;
pub const DEFAULT_PEAK_BYTES_PER_PIXEL: usize = 32;
pub const DEFAULT_MEMORY_BUDGET_FRACTION: f64 = 0.5;

const FALLBACK_CORES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Interactive,
    Background,
}

#[derive(Debug, Clone, Copy)]
pub struct WorkerPolicy {
    pub interactive_ratio: f64,
    pub background_ratio: f64,
    pub memory_budget_fraction: f64,
    pub hard_max_workers: usize,
    pub peak_bytes_per_pixel: usize,
}

impl Default for WorkerPolicy {
    fn default() -> Self {
        Self {
            interactive_ratio: DEFAULT_INTERACTIVE_RATIO,
            background_ratio: DEFAULT_BACKGROUND_RATIO,
            memory_budget_fraction: DEFAULT_MEMORY_BUDGET_FRACTION,
            hard_max_workers: DEFAULT_HARD_MAX_WORKERS,
            peak_bytes_per_pixel: DEFAULT_PEAK_BYTES_PER_PIXEL,
        }
    }
}

impl WorkerPolicy {
    pub fn ratio_for(&self, priority: Priority) -> f64 {
        match priority {
            Priority::Interactive => self.interactive_ratio,
            Priority::Background => self.background_ratio,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkerBudget {
    pub workers: usize,
    pub rationale: String,
}

pub fn logical_cores() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(FALLBACK_CORES)
}

pub fn plan_workers(
    requested: Option<usize>,
    policy: &WorkerPolicy,
    priority: Priority,
    frame_pixels: Option<usize>,
) -> WorkerBudget {
    plan_workers_with_facts(
        requested,
        policy,
        priority,
        frame_pixels,
        logical_cores(),
        available_memory_bytes(),
    )
}

/// Like [`plan_workers`] but uses caller-supplied system facts (core count and
/// available memory) instead of probing them. Callers on hot paths — e.g. queue
/// admission control that runs under a lock — should snapshot these once and use
/// this to avoid a `/proc` read or `sysctl` subprocess on every call.
pub fn plan_workers_with_facts(
    requested: Option<usize>,
    policy: &WorkerPolicy,
    priority: Priority,
    frame_pixels: Option<usize>,
    cores: usize,
    available_bytes: Option<u64>,
) -> WorkerBudget {
    let (workers, rationale) = compute_worker_count(
        requested,
        cores,
        frame_pixels,
        available_bytes,
        policy,
        policy.ratio_for(priority),
    );
    WorkerBudget { workers, rationale }
}

fn compute_worker_count(
    requested: Option<usize>,
    cores: usize,
    frame_pixels: Option<usize>,
    available_bytes: Option<u64>,
    policy: &WorkerPolicy,
    ratio: f64,
) -> (usize, String) {
    let hard_max = policy.hard_max_workers.max(1);

    if let Some(n) = requested {
        let workers = n.clamp(1, hard_max);
        return (workers, format!("explicit override: {} worker(s)", workers));
    }

    let cores = cores.max(1);
    let ratio = ratio.clamp(0.0, 1.0);
    let scaled = (((cores as f64 * ratio).round() as usize).max(1)).min(hard_max);

    let per_pixel = policy.peak_bytes_per_pixel.max(1) as u64;
    let mem_cap = match (frame_pixels, available_bytes) {
        (Some(px), Some(avail)) if px > 0 => {
            let per_frame = (px as u64).saturating_mul(per_pixel).max(1);
            let budget = (avail as f64 * policy.memory_budget_fraction) as u64;
            Some((budget / per_frame).max(1) as usize)
        }
        _ => None,
    };

    let mut workers = scaled;
    let mut rationale = format!("{} of {} core(s) at ratio {:.2}", scaled, cores, ratio);
    if let Some(cap) = mem_cap {
        if cap < workers {
            let mb_per_frame = frame_pixels
                .map(|px| (px as u64 * per_pixel) / (1024 * 1024))
                .unwrap_or(0);
            rationale = format!(
                "{}, capped to {} by memory (~{} MB/frame)",
                rationale, cap, mb_per_frame
            );
            workers = cap;
        } else {
            rationale = format!("{} (memory allows {})", rationale, cap);
        }
    }

    (workers.max(1), rationale)
}

pub fn parallel_index<F>(len: usize, workers: usize, f: F)
where
    F: Fn(usize) + Sync,
{
    if len == 0 {
        return;
    }

    let workers = workers.clamp(1, len);
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= len {
                        break;
                    }
                    f(i);
                }
            });
        }
    });
}

pub fn available_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let mut mem_total = None;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("MemAvailable:") {
                if let Some(kb) = parse_meminfo_kb(rest) {
                    return Some(kb);
                }
            } else if let Some(rest) = line.strip_prefix("MemTotal:") {
                mem_total = parse_meminfo_kb(rest);
            }
        }
        return mem_total;
    }

    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        let text = String::from_utf8(output.stdout).ok()?;
        return text.trim().parse::<u64>().ok().filter(|v| *v > 0);
    }

    #[allow(unreachable_code)]
    None
}

#[cfg(any(target_os = "linux", test))]
fn parse_meminfo_kb(rest: &str) -> Option<u64> {
    let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
    Some(kb.saturating_mul(1024))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> WorkerPolicy {
        WorkerPolicy::default()
    }

    #[test]
    fn explicit_override_wins_and_is_clamped() {
        let (workers, _) = compute_worker_count(
            Some(6),
            4,
            Some(50_000_000),
            Some(1_000_000_000),
            &policy(),
            0.5,
        );
        assert_eq!(workers, 6);

        let (workers, _) = compute_worker_count(Some(0), 8, None, None, &policy(), 1.0);
        assert_eq!(workers, 1);

        let (workers, _) = compute_worker_count(Some(9999), 8, None, None, &policy(), 1.0);
        assert_eq!(workers, DEFAULT_HARD_MAX_WORKERS);
    }

    #[test]
    fn priority_selects_core_ratio() {
        let p = policy();
        assert_eq!(
            p.ratio_for(Priority::Interactive),
            DEFAULT_INTERACTIVE_RATIO
        );
        assert_eq!(p.ratio_for(Priority::Background), DEFAULT_BACKGROUND_RATIO);

        let interactive =
            compute_worker_count(None, 16, None, None, &p, p.ratio_for(Priority::Interactive)).0;
        let background =
            compute_worker_count(None, 16, None, None, &p, p.ratio_for(Priority::Background)).0;
        assert_eq!(interactive, 8);
        assert_eq!(background, 4);
    }

    #[test]
    fn memory_ceiling_caps_workers() {
        let (workers, reason) = compute_worker_count(
            None,
            32,
            Some(50_000_000),
            Some(8 * 1024 * 1024 * 1024),
            &policy(),
            1.0,
        );
        assert_eq!(workers, 2, "reason: {reason}");
        assert!(reason.contains("memory"));
    }

    #[test]
    fn memory_ceiling_does_not_raise_above_core_budget() {
        let (workers, reason) = compute_worker_count(
            None,
            8,
            Some(20_000_000),
            Some(256 * 1024 * 1024 * 1024),
            &policy(),
            0.5,
        );
        assert_eq!(workers, 4);
        assert!(reason.contains("memory allows"));
    }

    #[test]
    fn parallel_index_covers_every_item_once() {
        use std::sync::atomic::AtomicU64;

        let len = 1000;
        let counters: Vec<AtomicU64> = (0..len).map(|_| AtomicU64::new(0)).collect();
        parallel_index(len, 8, |i| {
            counters[i].fetch_add(1, Ordering::Relaxed);
        });

        assert!(counters.iter().all(|c| c.load(Ordering::Relaxed) == 1));
        parallel_index(0, 4, |_| panic!("must not be called"));
    }

    #[test]
    fn parse_meminfo_line() {
        assert_eq!(parse_meminfo_kb(" 16384000 kB").unwrap(), 16384000 * 1024);
        assert_eq!(parse_meminfo_kb("       512 kB").unwrap(), 512 * 1024);
        assert!(parse_meminfo_kb(" not-a-number kB").is_none());
    }
}
