//! Statistical leak detection over a process's long term series.
//!
//! Plain "memory went up" produces constant false alarms. Garbage collected
//! runtimes sawtooth, caches fill then level off, and everything grows during
//! startup. So this looks at the floor of the series (the lowest value in each
//! time window, what survives a GC), tests whether that floor rises consistently
//! with Mann-Kendall, measures the rate with the outlier resistant Theil-Sen slope,
//! and checks whether the growth is still happening at the end of the window.

use crate::model::{Confidence, Pattern};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub t: f64,
    pub avg: f64,
    pub min: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Thresholds {
    /// Growth over the observed window needed before anything is reported as a leak.
    pub min_growth: f64,
    pub min_growth_pct: f64,
    pub min_rate_per_hour: f64,
    pub min_seconds: f64,
    /// Observed time that counts as well evidenced, for medium and high confidence.
    pub solid_seconds: f64,
}

#[derive(Clone, Debug)]
pub struct Trend {
    pub confidence: Confidence,
    pub pattern: Pattern,
    pub rate_per_hour: f64,
    pub growth: f64,
    pub growth_pct: f64,
    pub start_value: f64,
    pub current_value: f64,
    pub observed_secs: f64,
    pub z_score: f64,
    pub monotonicity: f64,
}

const MIN_POINTS: usize = 12;
const MIN_WINDOWS: usize = 8;
const MAX_WINDOWS: usize = 24;

/// Median of all pairwise slopes. One spike cannot move it far.
pub fn theil_sen(xs: &[f64], ys: &[f64]) -> f64 {
    let n = xs.len().min(ys.len());
    if n < 2 {
        return 0.0;
    }
    let mut slopes = Vec::with_capacity(n * (n - 1) / 2);
    for i in 0..n {
        for j in (i + 1)..n {
            let dx = xs[j] - xs[i];
            if dx > 0.0 {
                slopes.push((ys[j] - ys[i]) / dx);
            }
        }
    }
    median(&mut slopes)
}

fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

/// Mann-Kendall trend test, returned as a z score. Above about 2.33 is
/// significant at p < 0.01, one sided. Corrected for ties, since handle and
/// thread counts tend to sit on exact values.
pub fn mann_kendall(ys: &[f64]) -> f64 {
    let n = ys.len();
    if n < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..n {
        for j in (i + 1)..n {
            s += (ys[j] - ys[i]).signum() * f64::from((ys[j] - ys[i]).abs() > f64::EPSILON);
        }
    }
    let nf = n as f64;
    let mut sorted = ys.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut tie_term = 0.0;
    let mut run = 1.0;
    for pair in sorted.windows(2) {
        if (pair[1] - pair[0]).abs() <= f64::EPSILON {
            run += 1.0;
        } else {
            tie_term += run * (run - 1.0) * (2.0 * run + 5.0);
            run = 1.0;
        }
    }
    tie_term += run * (run - 1.0) * (2.0 * run + 5.0);
    let variance = (nf * (nf - 1.0) * (2.0 * nf + 5.0) - tie_term) / 18.0;
    if variance <= 0.0 {
        return 0.0;
    }
    if s > 0.0 {
        (s - 1.0) / variance.sqrt()
    } else if s < 0.0 {
        (s + 1.0) / variance.sqrt()
    } else {
        0.0
    }
}

/// Splits the series into equal stretches of time and takes the floor of each.
/// Returns (window midpoint, floor, mean of the window averages).
fn floors(points: &[Point]) -> Vec<(f64, f64, f64)> {
    let first = points[0].t;
    let last = points[points.len() - 1].t;
    let span = (last - first).max(1.0);
    let windows = (points.len() / 2).clamp(MIN_WINDOWS, MAX_WINDOWS);
    let width = span / windows as f64;
    let mut out = Vec::with_capacity(windows);
    for w in 0..windows {
        let lo = first + w as f64 * width;
        let hi = lo + width;
        let mut floor = f64::INFINITY;
        let mut sum = 0.0;
        let mut count = 0.0;
        for p in points {
            let inside = p.t >= lo && (p.t < hi || (w == windows - 1 && p.t <= hi));
            if inside {
                floor = floor.min(p.min);
                sum += p.avg;
                count += 1.0;
            }
        }
        if count > 0.0 {
            out.push((lo + width / 2.0, floor, sum / count));
        }
    }
    out
}

/// Classifies a series. `None` means nothing worth showing.
pub fn analyze(points: &[Point], process_start: f64, warmup_secs: f64, th: &Thresholds) -> Option<Trend> {
    // Startup growth is normal, so it never counts as evidence.
    let settled_from = process_start + warmup_secs;
    let settled: Vec<Point> = points.iter().copied().filter(|p| p.t >= settled_from).collect();
    if settled.len() < MIN_POINTS {
        return None;
    }
    let observed = settled[settled.len() - 1].t - settled[0].t;

    let windows = floors(&settled);
    if windows.len() < 4 {
        return None;
    }
    let times: Vec<f64> = windows.iter().map(|w| w.0).collect();
    let floor_values: Vec<f64> = windows.iter().map(|w| w.1).collect();

    let slope = theil_sen(&times, &floor_values);
    if slope <= 0.0 {
        return None;
    }
    let rate_per_hour = slope * 3600.0;
    let growth = slope * observed;
    if growth < th.min_growth * 0.25 {
        return None;
    }

    let head = windows.len().min(3);
    let mut start_slice = floor_values[..head].to_vec();
    let start_value = median(&mut start_slice);
    let current_value = settled[settled.len() - 1].avg;
    let growth_pct = growth / start_value.max(1.0) * 100.0;

    let z_score = mann_kendall(&floor_values);
    let rising = floor_values.windows(2).filter(|p| p[1] >= p[0]).count();
    let monotonicity = rising as f64 / (floor_values.len() - 1) as f64;

    // A cache that filled and stopped is not a leak: check the final quarter.
    let tail_start = (windows.len() * 3) / 4;
    let tail_slope = theil_sen(&times[tail_start.min(windows.len() - 3)..], &floor_values[tail_start.min(windows.len() - 3)..]);
    let plateaued = tail_slope < slope * 0.15;

    let pattern = pattern_of(&windows, growth, monotonicity, plateaued);

    let meets_size = growth >= th.min_growth && growth_pct >= th.min_growth_pct && rate_per_hour >= th.min_rate_per_hour;
    let significant = z_score >= 2.33;
    let long_enough = observed >= th.min_seconds;

    let confidence = if plateaued {
        if !meets_size {
            return None;
        }
        Confidence::Plateaued
    } else if meets_size && significant && long_enough {
        let solid = observed >= th.solid_seconds;
        if solid && z_score >= 3.0 && monotonicity >= 0.7 && growth >= th.min_growth * 2.0 {
            Confidence::High
        } else if solid && z_score >= 2.58 {
            Confidence::Medium
        } else {
            Confidence::Low
        }
    } else if z_score >= 1.64 {
        Confidence::Watching
    } else {
        return None;
    };

    Some(Trend {
        confidence,
        pattern,
        rate_per_hour,
        growth,
        growth_pct,
        start_value,
        current_value,
        observed_secs: observed,
        z_score,
        monotonicity,
    })
}

fn pattern_of(windows: &[(f64, f64, f64)], growth: f64, monotonicity: f64, plateaued: bool) -> Pattern {
    if plateaued {
        return Pattern::Plateau;
    }
    let floors: Vec<f64> = windows.iter().map(|w| w.1).collect();
    let swing: f64 = windows.iter().map(|w| w.2 - w.1).sum::<f64>() / windows.len() as f64;
    let step_size = growth / floors.len().max(1) as f64;
    // A steady climb sits about half a step above each window's floor. Well past
    // that, the series is swinging up and back down, as a GC heap does.
    if step_size > 0.0 && swing / step_size > 2.0 {
        return Pattern::SawtoothRisingFloor;
    }
    let flat = floors.windows(2).filter(|p| (p[1] - p[0]).abs() < step_size * 0.1).count();
    if monotonicity >= 0.7 && flat * 2 >= floors.len() - 1 {
        return Pattern::StairStep;
    }
    Pattern::SteadyClimb
}

/// Reduces a series for display without flattening its minimums away.
pub fn downsample(points: &[Point], max: usize) -> Vec<[f64; 2]> {
    if points.len() <= max || max == 0 {
        return points.iter().map(|p| [p.t, p.avg]).collect();
    }
    let step = points.len() as f64 / max as f64;
    (0..max)
        .map(|i| {
            let p = points[((i as f64 * step) as usize).min(points.len() - 1)];
            [p.t, p.avg]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: f64 = 1024.0 * 1024.0;

    fn th() -> Thresholds {
        Thresholds {
            min_growth: 64.0 * MB,
            min_growth_pct: 15.0,
            min_rate_per_hour: 8.0 * MB,
            min_seconds: 1800.0,
            solid_seconds: 7200.0,
        }
    }

    /// One point per minute for `hours`, with deterministic noise.
    fn series(hours: f64, f: impl Fn(f64) -> f64) -> Vec<Point> {
        let n = (hours * 60.0) as usize;
        (0..n)
            .map(|i| {
                let t = 100_000.0 + i as f64 * 60.0;
                let v = f(i as f64 / 60.0);
                Point { t, avg: v, min: v }
            })
            .collect()
    }

    fn noise(x: f64) -> f64 {
        ((x * 37.0).sin() + (x * 91.0).cos()) * 3.0 * MB
    }

    #[test]
    fn theil_sen_ignores_a_single_spike() {
        let xs: Vec<f64> = (0..20).map(f64::from).collect();
        let mut ys: Vec<f64> = xs.iter().map(|x| x * 2.0).collect();
        ys[10] = 10_000.0;
        assert!((theil_sen(&xs, &ys) - 2.0).abs() < 0.01);
    }

    #[test]
    fn mann_kendall_signs() {
        let up: Vec<f64> = (0..20).map(f64::from).collect();
        let down: Vec<f64> = up.iter().rev().copied().collect();
        assert!(mann_kendall(&up) > 3.0);
        assert!(mann_kendall(&down) < -3.0);
        assert_eq!(mann_kendall(&[5.0; 20]), 0.0);
    }

    #[test]
    fn steady_leak_is_high_confidence() {
        let pts = series(6.0, |h| 300.0 * MB + h * 60.0 * MB + noise(h));
        let t = analyze(&pts, 0.0, 600.0, &th()).unwrap();
        assert_eq!(t.confidence, Confidence::High);
        assert!((t.rate_per_hour / MB - 60.0).abs() < 6.0, "rate {}", t.rate_per_hour / MB);
        assert_eq!(t.pattern, Pattern::SteadyClimb);
    }

    #[test]
    fn flat_noisy_process_is_ignored() {
        let pts = series(6.0, |h| 500.0 * MB + noise(h));
        assert!(analyze(&pts, 0.0, 600.0, &th()).is_none());
    }

    #[test]
    fn sawtooth_without_rising_floor_is_ignored() {
        // A GC heap: climbs 200 MB then drops back, floor stays put.
        let mut pts = Vec::new();
        for i in 0..360 {
            let phase = (i % 30) as f64 / 30.0;
            let v = 400.0 * MB + phase * 200.0 * MB;
            let t = 100_000.0 + i as f64 * 60.0;
            pts.push(Point { t, avg: v, min: if i % 30 == 0 { 400.0 * MB } else { v } });
        }
        assert!(analyze(&pts, 0.0, 600.0, &th()).is_none());
    }

    #[test]
    fn sawtooth_with_rising_floor_is_caught() {
        let mut pts = Vec::new();
        for i in 0..360 {
            let hours = i as f64 / 60.0;
            let phase = (i % 30) as f64 / 30.0;
            let base = 400.0 * MB + hours * 40.0 * MB;
            let v = base + phase * 150.0 * MB;
            let t = 100_000.0 + i as f64 * 60.0;
            pts.push(Point { t, avg: v, min: if i % 30 == 0 { base } else { v } });
        }
        let t = analyze(&pts, 0.0, 600.0, &th()).unwrap();
        assert!(t.confidence >= Confidence::Medium, "{:?}", t.confidence);
        assert_eq!(t.pattern, Pattern::SawtoothRisingFloor);
    }

    #[test]
    fn cache_that_fills_then_stops_is_plateaued_not_a_leak() {
        let pts = series(8.0, |h| 300.0 * MB + (h.min(3.0)) * 100.0 * MB + noise(h));
        let t = analyze(&pts, 0.0, 600.0, &th()).unwrap();
        assert_eq!(t.confidence, Confidence::Plateaued);
        assert_eq!(t.pattern, Pattern::Plateau);
    }

    #[test]
    fn startup_growth_is_excluded_by_warmup() {
        // Climbs for 10 minutes, then flat.
        let pts = series(4.0, |h| 200.0 * MB + (h * 6.0).min(1.0) * 400.0 * MB);
        let start = pts[0].t;
        assert!(analyze(&pts, start, 900.0, &th()).is_none());
    }

    #[test]
    fn too_little_data_is_never_reported_as_a_leak() {
        let pts = series(0.25, |h| 200.0 * MB + h * 800.0 * MB);
        match analyze(&pts, 0.0, 600.0, &th()) {
            None => {}
            Some(t) => assert_eq!(t.confidence, Confidence::Watching),
        }
    }

    #[test]
    fn small_growth_is_not_a_leak() {
        let pts = series(6.0, |h| 300.0 * MB + h * 1.0 * MB);
        assert!(analyze(&pts, 0.0, 600.0, &th()).is_none());
    }

    #[test]
    fn handle_leak_with_exact_integer_values() {
        let pts: Vec<Point> = (0..240)
            .map(|i| {
                let v = (1000 + i * 5) as f64;
                Point { t: 100_000.0 + i as f64 * 60.0, avg: v, min: v }
            })
            .collect();
        let handles = Thresholds {
            min_growth: 500.0,
            min_growth_pct: 25.0,
            min_rate_per_hour: 50.0,
            min_seconds: 1800.0,
            solid_seconds: 7200.0,
        };
        let t = analyze(&pts, 0.0, 600.0, &handles).unwrap();
        assert_eq!(t.confidence, Confidence::High);
    }

    #[test]
    fn stair_step_growth_is_named() {
        let pts = series(8.0, |h| 300.0 * MB + (h.floor()) * 80.0 * MB);
        let t = analyze(&pts, 0.0, 600.0, &th()).unwrap();
        assert!(t.confidence >= Confidence::Medium, "{:?}", t.confidence);
        assert_eq!(t.pattern, Pattern::StairStep);
    }

    #[test]
    fn falling_memory_is_ignored() {
        let pts = series(6.0, |h| 2000.0 * MB - h * 100.0 * MB);
        assert!(analyze(&pts, 0.0, 600.0, &th()).is_none());
    }

    #[test]
    fn downsample_is_bounded() {
        let pts = series(10.0, |h| h);
        assert_eq!(downsample(&pts, 60).len(), 60);
        assert_eq!(downsample(&pts[..10], 60).len(), 10);
    }
}
