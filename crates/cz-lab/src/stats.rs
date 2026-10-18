//! Statistics over the rows a run wrote (ADR-0015 D2): the CSV tables, the
//! metrics a contrast reads from them, the paired statistics of EXP-004's
//! `summarize.py` (same-seed differences, 95% bootstrap interval, d_z,
//! ratio of means, seeds by sign) and the distribution of a metric over a
//! cell's seeds.
//!
//! The bootstrap draws from [`BootRng`], a counter generator of the
//! kernel's SplitMix family seeded [`BOOT_SEED`]: the same rows give the
//! same interval on every machine, native or wasm. Against `summarize.py`
//! the mean and d_z agree to rounding; the interval's end points differ by
//! the Monte Carlo error of two different random sequences (about ±1e-4 on
//! EXP-004's contrasts; `tests/stats.rs`).
//!
//! Floating point is used here, in the statistics, and nowhere in the
//! kernel (ADR-0003).

use crate::design::{Columns, Contrast, Metric};
use crate::Error;
use serde::{Deserialize, Serialize};
use simulation_core::rng::mix64;

/// Bootstrap resamples.
pub const BOOT: usize = 10_000;

/// Seed of the bootstrap's generator (EXP-004's `random.Random(20261018)`).
pub const BOOT_SEED: u64 = 20261018;

/// A CSV as `cz sweep` writes it: a header and rows of plain fields (no
/// quoting; the sweep never writes a comma inside a field).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

fn parse_f64(s: &str) -> Option<f64> {
    if s.is_empty() {
        None
    } else {
        s.parse().ok()
    }
}

fn parse_u64(s: &str) -> Option<u64> {
    s.parse().ok()
}

impl Table {
    /// Parses a whole CSV file.
    pub fn parse(text: &str) -> Result<Table, Error> {
        let mut lines = text.lines();
        let header = lines.next().ok_or("empty CSV")?;
        let rows: Vec<String> = lines
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        Self::from_lines(header, &rows)
    }

    /// A table from a header line and row lines (what a [`crate::Rows`]
    /// holds).
    pub fn from_lines(header: &str, rows: &[String]) -> Result<Table, Error> {
        let columns: Vec<String> = header.split(',').map(str::to_string).collect();
        let mut out = Vec::with_capacity(rows.len());
        for (i, line) in rows.iter().enumerate() {
            let fields: Vec<String> = line.split(',').map(str::to_string).collect();
            if fields.len() != columns.len() {
                return Err(Error(format!(
                    "CSV row {}: {} fields, the header has {}",
                    i + 1,
                    fields.len(),
                    columns.len()
                )));
            }
            out.push(fields);
        }
        Ok(Table { columns, rows: out })
    }

    pub fn col(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }

    pub fn require(&self, name: &str) -> Result<usize, Error> {
        self.col(name)
            .ok_or_else(|| Error(format!("no column {name:?} in {}", self.columns.join(","))))
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// One cell's `summary` and `series` tables.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellTables {
    pub summary: Table,
    pub series: Table,
}

impl CellTables {
    /// The seeds of the summary, ascending.
    pub fn seeds(&self) -> Result<Vec<u64>, Error> {
        let si = self.summary.require("seed")?;
        let mut out: Vec<u64> = self
            .summary
            .rows
            .iter()
            .filter_map(|r| parse_u64(&r[si]))
            .collect();
        out.sort_unstable();
        out.dedup();
        Ok(out)
    }

    /// The seed's summary row, if the seed ran.
    pub fn summary_row(&self, seed: u64) -> Result<Option<&[String]>, Error> {
        let si = self.summary.require("seed")?;
        Ok(self
            .summary
            .rows
            .iter()
            .find(|r| parse_u64(&r[si]) == Some(seed))
            .map(Vec::as_slice))
    }

    /// Whether the seed's world went extinct (`extinct_at` is a tick, not
    /// `-`); `None` if the seed did not run.
    pub fn extinct(&self, seed: u64) -> Result<Option<bool>, Error> {
        let ei = self.summary.require("extinct_at")?;
        Ok(self.summary_row(seed)?.map(|r| r[ei] != "-"))
    }

    fn series_window(
        &self,
        seed: u64,
        window: (u64, u64),
    ) -> Result<impl Iterator<Item = &Vec<String>>, Error> {
        let si = self.series.require("seed")?;
        let ti = self.series.require("tick")?;
        Ok(self.series.rows.iter().filter(move |r| {
            parse_u64(&r[si]) == Some(seed)
                && parse_u64(&r[ti]).is_some_and(|t| t > window.0 && t <= window.1)
        }))
    }

    fn series_sum(&self, seed: u64, cols: &Columns, window: (u64, u64)) -> Result<f64, Error> {
        let idx: Vec<usize> = cols
            .0
            .iter()
            .map(|c| self.series.require(c))
            .collect::<Result<_, _>>()?;
        let mut sum = 0.0;
        for row in self.series_window(seed, window)? {
            for &i in &idx {
                sum += parse_f64(&row[i]).unwrap_or(0.0);
            }
        }
        Ok(sum)
    }

    fn summary_sum(&self, row: &[String], cols: &Columns) -> Result<Option<f64>, Error> {
        let mut sum = 0.0;
        for c in &cols.0 {
            match parse_f64(&row[self.summary.require(c)?]) {
                Some(v) => sum += v,
                None => return Ok(None),
            }
        }
        Ok(Some(sum))
    }

    /// The metric's value for one seed: `None` when it is undefined (a
    /// zero denominator, an empty cell, no rows in the window, a seed that
    /// did not run); an error when a column does not exist.
    pub fn metric(
        &self,
        seed: u64,
        metric: &Metric,
        window: (u64, u64),
    ) -> Result<Option<f64>, Error> {
        let finite = |v: f64| v.is_finite().then_some(v);
        Ok(match metric {
            Metric::Ratio { num, den } => {
                let d = self.series_sum(seed, den, window)?;
                if d == 0.0 {
                    None
                } else {
                    finite(self.series_sum(seed, num, window)? / d)
                }
            }
            Metric::Mean(col) => {
                let i = self.series.require(col)?;
                let (mut sum, mut n) = (0.0, 0u64);
                for row in self.series_window(seed, window)? {
                    if let Some(v) = parse_f64(&row[i]) {
                        sum += v;
                        n += 1;
                    }
                }
                (n > 0).then(|| sum / n as f64).and_then(finite)
            }
            Metric::PerTick(col) => {
                let ticks = window.1.saturating_sub(window.0);
                if ticks == 0 {
                    None
                } else {
                    finite(
                        self.series_sum(seed, &Columns(vec![col.clone()]), window)? / ticks as f64,
                    )
                }
            }
            Metric::Summary(col) => {
                let i = self.summary.require(col)?;
                self.summary_row(seed)?
                    .and_then(|r| parse_f64(&r[i]))
                    .and_then(finite)
            }
            Metric::SummaryRatio { num, den } => match self.summary_row(seed)? {
                None => None,
                Some(row) => match (self.summary_sum(row, num)?, self.summary_sum(row, den)?) {
                    (Some(n), Some(d)) if d != 0.0 => finite(n / d),
                    _ => None,
                },
            },
        })
    }
}

/// A counter generator of the kernel's family (`mix64` over a key and a
/// counter, as `RngStream`), for the bootstrap only.
#[derive(Clone, Debug)]
pub struct BootRng {
    key: u64,
    counter: u64,
}

impl BootRng {
    pub fn new(seed: u64) -> Self {
        Self {
            key: mix64(seed),
            counter: 0,
        }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let v = mix64(self.key ^ mix64(self.counter));
        self.counter += 1;
        v
    }

    /// Uniform integer in `0..n` without modulo bias (Lemire's method, as
    /// `RngStream::below`). Panics if `n == 0`.
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "BootRng::below(0)");
        let threshold = n.wrapping_neg() % n;
        loop {
            let m = (self.next_u64() as u128) * (n as u128);
            if (m as u64) >= threshold {
                return (m >> 64) as u64;
            }
        }
    }
}

/// The 95% bootstrap interval of the mean of `d`: `boot` resamples of
/// `d.len()` draws with replacement, their means sorted, the values at the
/// 2.5th and 97.5th percentiles (`summarize.py`'s `boots[int(0.025 *
/// BOOT)]` and `boots[int(0.975 * BOOT) - 1]`). `None` for no data.
pub fn bootstrap_ci(d: &[f64], boot: usize, seed: u64) -> Option<(f64, f64)> {
    let n = d.len();
    if n == 0 || boot == 0 {
        return None;
    }
    let mut rng = BootRng::new(seed);
    let mut means = Vec::with_capacity(boot);
    for _ in 0..boot {
        let mut s = 0.0;
        for _ in 0..n {
            s += d[rng.below(n as u64) as usize];
        }
        means.push(s / n as f64);
    }
    means.sort_by(f64::total_cmp);
    let lo = means[boot * 25 / 1000];
    let hi = means[(boot * 975 / 1000).saturating_sub(1)];
    Some((lo, hi))
}

/// The paired statistics of one contrast.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PairedStats {
    pub name: String,
    pub treat: String,
    pub control: String,
    pub metric: Metric,
    /// `(from, to]` of the series rows read (the summary's window for
    /// `summary` metrics).
    pub window: (u64, u64),
    /// Pairs used: seeds that ran in both cells, went extinct in neither,
    /// and have the metric defined in both.
    pub n: usize,
    /// Seeds common to both cells that were left out.
    pub dropped: usize,
    /// Mean of `treat − control`; NaN without pairs.
    pub mean: f64,
    /// 95% bootstrap interval of the mean.
    pub lo: f64,
    pub hi: f64,
    /// `mean / sd(differences, n − 1)`; NaN when `n < 2` or the sd is 0.
    pub d_z: f64,
    /// `mean_treat / mean_control`; NaN when the control's mean is 0.
    pub ratio: f64,
    pub mean_treat: f64,
    pub mean_control: f64,
    /// Pairs with `treat < control` / `treat > control`.
    pub negative: usize,
    pub positive: usize,
    pub boot: usize,
    pub boot_seed: u64,
    /// `(seed, treat − control)` of every pair used.
    pub pairs: Vec<(u64, f64)>,
}

impl PairedStats {
    /// `summarize.py`'s `ci_text`: `mean [lo, hi], d_z x, n k (dropped j)`.
    pub fn ci_text(&self, digits: usize) -> String {
        if self.n == 0 {
            return "—".to_string();
        }
        let mut s = format!(
            "{} [{}, {}], d_z {}, n {}",
            fmt(self.mean, digits),
            fmt(self.lo, digits),
            fmt(self.hi, digits),
            fmt(self.d_z, 2),
            self.n
        );
        if self.dropped > 0 {
            s.push_str(&format!(" (dropped {})", self.dropped));
        }
        s
    }
}

/// A number with `digits` decimals, or `—` when it is not finite.
pub fn fmt(x: f64, digits: usize) -> String {
    if x.is_finite() {
        format!("{x:.digits$}")
    } else {
        "—".to_string()
    }
}

/// The paired statistics of `contrast` over the seeds both cells ran.
pub fn paired_stats(
    treat: &CellTables,
    control: &CellTables,
    contrast: &Contrast,
    default_window: (u64, u64),
) -> Result<PairedStats, Error> {
    let window = contrast.window.unwrap_or(default_window);
    let control_seeds = control.seeds()?;
    let seeds: Vec<u64> = treat
        .seeds()?
        .into_iter()
        .filter(|s| control_seeds.contains(s))
        .collect();
    let mut pairs = Vec::new();
    let mut dropped = 0;
    for &seed in &seeds {
        let extinct =
            treat.extinct(seed)?.unwrap_or(true) || control.extinct(seed)?.unwrap_or(true);
        let a = treat.metric(seed, &contrast.metric, window)?;
        let b = control.metric(seed, &contrast.metric, window)?;
        match (extinct, a, b) {
            (false, Some(a), Some(b)) => pairs.push((seed, a, b)),
            _ => dropped += 1,
        }
    }
    let n = pairs.len();
    let nan = f64::NAN;
    let mut out = PairedStats {
        name: contrast.name.clone(),
        treat: contrast.treat.clone(),
        control: contrast.control.clone(),
        metric: contrast.metric.clone(),
        window,
        n,
        dropped,
        mean: nan,
        lo: nan,
        hi: nan,
        d_z: nan,
        ratio: nan,
        mean_treat: nan,
        mean_control: nan,
        negative: 0,
        positive: 0,
        boot: BOOT,
        boot_seed: BOOT_SEED,
        pairs: pairs.iter().map(|&(s, a, b)| (s, a - b)).collect(),
    };
    if n == 0 {
        return Ok(out);
    }
    let d: Vec<f64> = out.pairs.iter().map(|&(_, x)| x).collect();
    let nf = n as f64;
    let mean = d.iter().sum::<f64>() / nf;
    let sd = if n > 1 {
        (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (nf - 1.0)).sqrt()
    } else {
        nan
    };
    let (lo, hi) = bootstrap_ci(&d, BOOT, BOOT_SEED).expect("n > 0");
    let mean_treat = pairs.iter().map(|&(_, a, _)| a).sum::<f64>() / nf;
    let mean_control = pairs.iter().map(|&(_, _, b)| b).sum::<f64>() / nf;
    out.mean = mean;
    out.lo = lo;
    out.hi = hi;
    out.d_z = if sd.is_finite() && sd != 0.0 {
        mean / sd
    } else {
        nan
    };
    out.ratio = if mean_control != 0.0 {
        mean_treat / mean_control
    } else {
        nan
    };
    out.mean_treat = mean_treat;
    out.mean_control = mean_control;
    out.negative = d.iter().filter(|&&x| x < 0.0).count();
    out.positive = d.iter().filter(|&&x| x > 0.0).count();
    Ok(out)
}

/// Where a metric's values over a cell's seeds lie.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Distribution {
    pub n: usize,
    pub mean: f64,
    /// Sample standard deviation (n − 1); NaN for n < 2.
    pub sd: f64,
    pub min: f64,
    /// Quartiles by linear interpolation at `p · (n − 1)`.
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub max: f64,
}

fn quantile(sorted: &[f64], p: f64) -> f64 {
    let pos = p * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - lo as f64;
    sorted[lo] + (sorted[hi] - sorted[lo]) * frac
}

/// The distribution of the finite values; `None` without one.
pub fn distribution(values: &[f64]) -> Option<Distribution> {
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    let mean = v.iter().sum::<f64>() / n as f64;
    let sd = if n > 1 {
        (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt()
    } else {
        f64::NAN
    };
    Some(Distribution {
        n,
        mean,
        sd,
        min: v[0],
        q1: quantile(&v, 0.25),
        median: quantile(&v, 0.5),
        q3: quantile(&v, 0.75),
        max: v[n - 1],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bootstrap_is_deterministic_and_brackets_the_mean() {
        let d: Vec<f64> = (0..30).map(|i| (i as f64) * 0.1 - 1.0).collect();
        let a = bootstrap_ci(&d, BOOT, BOOT_SEED).unwrap();
        let b = bootstrap_ci(&d, BOOT, BOOT_SEED).unwrap();
        assert_eq!(a, b);
        let mean = d.iter().sum::<f64>() / d.len() as f64;
        assert!(a.0 < mean && mean < a.1, "{a:?} around {mean}");
        assert_ne!(bootstrap_ci(&d, BOOT, 1).unwrap(), a);
        assert_eq!(bootstrap_ci(&[], BOOT, BOOT_SEED), None);
        assert_eq!(bootstrap_ci(&[2.5], BOOT, BOOT_SEED), Some((2.5, 2.5)));
    }

    #[test]
    fn below_is_in_range_and_roughly_uniform() {
        let mut r = BootRng::new(BOOT_SEED);
        let mut counts = [0u32; 7];
        for _ in 0..70_000 {
            counts[r.below(7) as usize] += 1;
        }
        for c in counts {
            assert!((9_500..10_500).contains(&c), "{counts:?}");
        }
    }

    #[test]
    fn quantiles_interpolate_linearly() {
        let d = distribution(&[4.0, 1.0, 3.0, 2.0, f64::NAN]).unwrap();
        assert_eq!(d.n, 4);
        assert_eq!((d.min, d.max), (1.0, 4.0));
        assert_eq!(d.median, 2.5);
        assert_eq!(d.q1, 1.75);
        assert_eq!(d.q3, 3.25);
        assert!((d.sd - (5.0f64 / 3.0).sqrt()).abs() < 1e-12);
        assert!(distribution(&[1.0]).unwrap().sd.is_nan());
        assert_eq!(distribution(&[]), None);
    }
}
