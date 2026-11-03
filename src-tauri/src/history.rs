//! Long term series with bounded memory.
//!
//! Each series keeps at most `CAPACITY` buckets. When it fills up, neighbours are
//! merged and the bucket span doubles, so a process's whole lifetime stays covered
//! at falling resolution. Minimums survive merging, which is what leak detection
//! reads, so old data stays useful instead of being averaged flat.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::leaks::Point;
use crate::model::{ExitedInfo, Metric};

pub const CAPACITY: usize = 480;
pub const FINE_CAPACITY: usize = 180;
const EXITED_CAPACITY: usize = 500;

pub trait Mergeable: Copy {
    fn merge(self, other: Self) -> Self;
    fn t(&self) -> u64;
}

fn weighted(a: u32, ak: u32, b: u32, bk: u32) -> u32 {
    let total = u64::from(ak) + u64::from(bk);
    if total == 0 {
        return 0;
    }
    ((u64::from(a) * u64::from(ak) + u64::from(b) * u64::from(bk)) / total) as u32
}

/// One process or app sample. Memory is held in KiB so 32 bits is plenty, which
/// keeps thousands of buckets small in memory and on disk.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct Bucket {
    pub t: u64,
    pub p: u32,
    pub pm: u32,
    pub px: u32,
    pub w: u32,
    pub h: u32,
    pub hm: u32,
    pub n: u32,
    pub nm: u32,
    pub g: u32,
    pub gm: u32,
    pub u: u32,
    pub um: u32,
    pub c: f32,
    pub k: u32,
}

impl Mergeable for Bucket {
    fn merge(self, o: Self) -> Self {
        let total = (self.k + o.k).max(1) as f32;
        Bucket {
            t: self.t.min(o.t),
            p: weighted(self.p, self.k, o.p, o.k),
            pm: self.pm.min(o.pm),
            px: self.px.max(o.px),
            w: weighted(self.w, self.k, o.w, o.k),
            h: weighted(self.h, self.k, o.h, o.k),
            hm: self.hm.min(o.hm),
            n: weighted(self.n, self.k, o.n, o.k),
            nm: self.nm.min(o.nm),
            g: weighted(self.g, self.k, o.g, o.k),
            gm: self.gm.min(o.gm),
            u: weighted(self.u, self.k, o.u, o.k),
            um: self.um.min(o.um),
            c: (self.c * self.k as f32 + o.c * o.k as f32) / total,
            k: self.k + o.k,
        }
    }
    fn t(&self) -> u64 {
        self.t
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct SysBucket {
    pub t: u64,
    pub used: u32,
    pub commit: u32,
    pub nonpaged: u32,
    pub nonpaged_min: u32,
    pub cache: u32,
    pub cpu: f32,
    pub k: u32,
}

impl Mergeable for SysBucket {
    fn merge(self, o: Self) -> Self {
        let total = (self.k + o.k).max(1) as f32;
        SysBucket {
            t: self.t.min(o.t),
            used: weighted(self.used, self.k, o.used, o.k),
            commit: weighted(self.commit, self.k, o.commit, o.k),
            nonpaged: weighted(self.nonpaged, self.k, o.nonpaged, o.k),
            nonpaged_min: self.nonpaged_min.min(o.nonpaged_min),
            cache: weighted(self.cache, self.k, o.cache, o.k),
            cpu: (self.cpu * self.k as f32 + o.cpu * o.k as f32) / total,
            k: self.k + o.k,
        }
    }
    fn t(&self) -> u64 {
        self.t
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Series<T> {
    pub buckets: Vec<T>,
    pub span: u64,
    #[serde(skip)]
    open: Option<T>,
}

impl<T: Mergeable> Series<T> {
    pub fn new(span: u64) -> Self {
        Series {
            buckets: Vec::new(),
            span: span.max(1),
            open: None,
        }
    }

    pub fn push(&mut self, sample: T) {
        let now = sample.t();
        let merged = match self.open.take() {
            Some(open) => open.merge(sample),
            None => sample,
        };
        if now.saturating_sub(merged.t()) + 1 >= self.span {
            self.close(merged);
        } else {
            self.open = Some(merged);
        }
    }

    fn close(&mut self, bucket: T) {
        self.buckets.push(bucket);
        if self.buckets.len() >= CAPACITY {
            let compacted = self
                .buckets
                .chunks(2)
                .map(|pair| match pair {
                    [a, b] => a.merge(*b),
                    [a] => *a,
                    _ => unreachable!("chunks(2) yields one or two items"),
                })
                .collect();
            self.buckets = compacted;
            self.span *= 2;
        }
    }

    /// Closed buckets plus the one being filled, so fresh data is visible.
    pub fn points(&self) -> Vec<T> {
        let mut out = self.buckets.clone();
        if let Some(open) = self.open {
            out.push(open);
        }
        out
    }

    pub fn last_t(&self) -> u64 {
        self.open.map(|o| o.t()).or_else(|| self.buckets.last().map(Mergeable::t)).unwrap_or(0)
    }
}

pub struct ProcHist {
    pub series: Series<Bucket>,
    pub fine: VecDeque<Bucket>,
    pub peak_private: u64,
}

impl ProcHist {
    pub fn new(span: u64) -> Self {
        ProcHist {
            series: Series::new(span),
            fine: VecDeque::with_capacity(16),
            peak_private: 0,
        }
    }

    pub fn push(&mut self, sample: Bucket) {
        self.peak_private = self.peak_private.max(u64::from(sample.px) * 1024);
        self.series.push(sample);
        if self.fine.len() == FINE_CAPACITY {
            self.fine.pop_front();
        }
        self.fine.push_back(sample);
    }

    /// How long CPU has stayed under `threshold`, walking back from now.
    pub fn idle_secs(&self, threshold: f32, now: u64) -> u64 {
        let mut since = now;
        for b in self.fine.iter().rev() {
            if b.c >= threshold {
                return now.saturating_sub(since);
            }
            since = b.t;
        }
        for b in self.series.buckets.iter().rev() {
            if b.t >= since {
                continue;
            }
            if b.c >= threshold {
                return now.saturating_sub(since);
            }
            since = b.t;
        }
        now.saturating_sub(since)
    }
}

pub struct AppHist {
    pub series: Series<Bucket>,
    pub fine: VecDeque<Bucket>,
}

#[derive(Serialize, Deserialize, Default)]
struct PersistFile {
    version: u32,
    saved_at: u64,
    monitor_since: u64,
    procs: HashMap<String, PersistedProc>,
    apps: HashMap<String, Series<Bucket>>,
    system: Option<Series<SysBucket>>,
    exited: Vec<ExitedInfo>,
}

#[derive(Serialize, Deserialize)]
struct PersistedProc {
    series: Series<Bucket>,
    peak_private: u64,
}

const WEEK: u64 = 7 * 24 * 3600;

pub struct History {
    pub procs: HashMap<String, ProcHist>,
    pub apps: HashMap<String, AppHist>,
    pub system: Series<SysBucket>,
    pub system_fine: VecDeque<SysBucket>,
    pub exited: VecDeque<ExitedInfo>,
    pub monitor_since: u64,
    pub span: u64,
}

impl History {
    pub fn new(span: u64, now: u64) -> Self {
        History {
            procs: HashMap::new(),
            apps: HashMap::new(),
            system: Series::new(span),
            system_fine: VecDeque::with_capacity(FINE_CAPACITY),
            exited: VecDeque::new(),
            monitor_since: now,
            span,
        }
    }

    pub fn push_system(&mut self, sample: SysBucket) {
        self.system.push(sample);
        if self.system_fine.len() == FINE_CAPACITY {
            self.system_fine.pop_front();
        }
        self.system_fine.push_back(sample);
    }

    pub fn push_app(&mut self, label: &str, sample: Bucket) {
        let span = self.span;
        let app = self.apps.entry(label.to_string()).or_insert_with(|| AppHist {
            series: Series::new(span),
            fine: VecDeque::with_capacity(16),
        });
        app.series.push(sample);
        if app.fine.len() == FINE_CAPACITY {
            app.fine.pop_front();
        }
        app.fine.push_back(sample);
    }

    pub fn record_exit(&mut self, info: ExitedInfo) {
        if self.exited.len() == EXITED_CAPACITY {
            self.exited.pop_front();
        }
        self.exited.push_back(info);
    }

    pub fn to_json(&self, now: u64) -> Option<String> {
        let file = PersistFile {
            version: 1,
            saved_at: now,
            monitor_since: self.monitor_since,
            // Short lived and tiny processes are not worth carrying across restarts.
            procs: self
                .procs
                .iter()
                .filter(|(_, h)| h.series.buckets.len() >= 4 && h.peak_private >= 20 * 1024 * 1024)
                .map(|(k, h)| {
                    (
                        k.clone(),
                        PersistedProc {
                            series: h.series.clone(),
                            peak_private: h.peak_private,
                        },
                    )
                })
                .collect(),
            apps: self
                .apps
                .iter()
                .filter(|(_, a)| now.saturating_sub(a.series.last_t()) < WEEK)
                .map(|(k, a)| (k.clone(), a.series.clone()))
                .collect(),
            system: Some(self.system.clone()),
            exited: self.exited.iter().cloned().collect(),
        };
        serde_json::to_string(&file).ok()
    }

    /// Restores from disk. Anything unreadable just means starting fresh, so a
    /// corrupt file can never stop the app from opening.
    pub fn restore(&mut self, json: &str, now: u64) {
        let Ok(file) = serde_json::from_str::<PersistFile>(json) else {
            return;
        };
        if file.version != 1 || now.saturating_sub(file.saved_at) > WEEK {
            return;
        }
        self.monitor_since = file.monitor_since.min(now);
        for (key, p) in file.procs {
            self.procs.insert(
                key,
                ProcHist {
                    series: p.series,
                    fine: VecDeque::with_capacity(16),
                    peak_private: p.peak_private,
                },
            );
        }
        for (label, series) in file.apps {
            self.apps.insert(
                label,
                AppHist {
                    series,
                    fine: VecDeque::with_capacity(16),
                },
            );
        }
        if let Some(system) = file.system {
            self.system = system;
        }
        self.exited = file.exited.into();
    }
}

pub fn metric_points(buckets: &[Bucket], metric: Metric) -> Vec<Point> {
    const KIB: f64 = 1024.0;
    buckets
        .iter()
        .map(|b| {
            let t = b.t as f64;
            match metric {
                Metric::Memory => Point { t, avg: f64::from(b.p) * KIB, min: f64::from(b.pm) * KIB },
                Metric::Handles => Point { t, avg: f64::from(b.h), min: f64::from(b.hm) },
                Metric::Threads => Point { t, avg: f64::from(b.n), min: f64::from(b.nm) },
                Metric::Gdi => Point { t, avg: f64::from(b.g), min: f64::from(b.gm) },
                Metric::UserObjects => Point { t, avg: f64::from(b.u), min: f64::from(b.um) },
                Metric::NonpagedPool => Point { t, avg: 0.0, min: 0.0 },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(t: u64, p: u32) -> Bucket {
        Bucket { t, p, pm: p, px: p, k: 1, ..Default::default() }
    }

    #[test]
    fn samples_close_into_buckets_at_the_span() {
        let mut s = Series::new(30);
        for i in 0..60u64 {
            s.push(sample(1000 + i * 2, 100));
        }
        // 120 seconds of samples every 2s at a 30s span: three closed, one still open.
        assert_eq!(s.buckets.len(), 3);
        assert_eq!(s.points().len(), 4);
    }

    #[test]
    fn compaction_bounds_memory_and_keeps_the_whole_timeline() {
        let mut s = Series::new(10);
        let first = 5000u64;
        let mut t = first;
        for i in 0..20_000u32 {
            s.push(sample(t, 100 + i));
            t += 10;
        }
        assert!(s.buckets.len() < CAPACITY);
        assert!(s.span > 10);
        assert_eq!(s.buckets[0].t, first);
    }

    #[test]
    fn merging_preserves_extremes_and_weights_averages() {
        let a = Bucket { t: 10, p: 100, pm: 90, px: 110, k: 3, ..Default::default() };
        let b = Bucket { t: 20, p: 200, pm: 150, px: 400, k: 1, ..Default::default() };
        let m = a.merge(b);
        assert_eq!((m.t, m.pm, m.px, m.k), (10, 90, 400, 4));
        assert_eq!(m.p, 125);
    }

    #[test]
    fn persistence_round_trips_and_survives_garbage() {
        let mut h = History::new(10, 1000);
        let mut proc_hist = ProcHist::new(10);
        for i in 0..100u64 {
            proc_hist.push(sample(1000 + i * 10, 50 * 1024));
        }
        let closed = proc_hist.series.buckets.len();
        h.procs.insert("1-2".into(), proc_hist);
        let json = h.to_json(2000).unwrap();

        let mut restored = History::new(10, 3000);
        restored.restore(&json, 3000);
        assert_eq!(restored.procs["1-2"].series.buckets.len(), closed);
        assert_eq!(restored.monitor_since, 1000);

        let mut fresh = History::new(10, 3000);
        fresh.restore("{not json", 3000);
        assert!(fresh.procs.is_empty());
    }

    #[test]
    fn stale_state_files_are_ignored() {
        let h = History::new(10, 1000);
        let json = h.to_json(1000).unwrap();
        let mut later = History::new(10, 1000 + WEEK + 10);
        later.restore(&json, 1000 + WEEK + 10);
        assert_eq!(later.monitor_since, 1000 + WEEK + 10);
    }

    #[test]
    fn idle_time_stops_at_the_last_busy_sample() {
        let mut h = ProcHist::new(30);
        for i in 0..50u64 {
            let mut s = sample(1000 + i * 2, 100);
            s.c = if i < 10 { 20.0 } else { 0.0 };
            h.push(s);
        }
        let now = 1000 + 49 * 2;
        // Busy until index 9, so it has been idle since the sample at index 10.
        assert_eq!(h.idle_secs(1.0, now), now - (1000 + 10 * 2));
    }
}
