//! `RUN.json` (ADR-0015 D2, `lab_result: 1`): what a run was — the design
//! it ran, which of its cells and seeds, the kernel's commit and the
//! machine, when it started and finished, and one record per world with
//! the `dynamics_hash` it reached. `RUN.log`'s facts (commit, binary,
//! load, wall clock) live here now; the runner's standard output repeats
//! them.

use crate::design::Design;
use crate::run::CellRun;
use crate::Error;
use serde::{Deserialize, Serialize};

/// `"lab_result"` of a manifest.
pub const LAB_RESULT_FORMAT: u32 = 1;

/// Which part of the design a run covered (`cz lab run --cells / --seeds`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    /// The cells run, or `None` for all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cells: Option<Vec<String>>,
    /// `[start, end)` of the seeds run, or `None` for each cell's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seeds: Option<(u64, u64)>,
}

/// Where and with what the run happened. Every field is best effort: the
/// runner fills what it can read and leaves the rest `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    /// `git rev-parse HEAD` at run time.
    #[serde(default)]
    pub commit: Option<String>,
    /// Paths `git status --porcelain` listed (0 = a clean tree).
    #[serde(default)]
    pub worktree_modified: Option<usize>,
    /// The CPU's name.
    #[serde(default)]
    pub machine: Option<String>,
    #[serde(default)]
    pub os: Option<String>,
    #[serde(default)]
    pub cores: Option<usize>,
    #[serde(default)]
    pub memory_gb: Option<u64>,
    /// The compiler that built the runner.
    #[serde(default)]
    pub rustc: Option<String>,
    /// SHA-256 of the runner binary.
    #[serde(default)]
    pub binary_sha256: Option<String>,
    /// Load averages at start / end (`uptime`).
    #[serde(default)]
    pub load_at_start: Option<String>,
    #[serde(default)]
    pub load_at_end: Option<String>,
}

/// One world of the run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldRecord {
    pub cell: String,
    pub seed: u64,
    pub ticks: u64,
    /// `World::dynamics_hash` of the final state, `0x` + 16 hex digits
    /// (a string: JavaScript has no exact u64).
    pub dynamics_hash: String,
    pub state_hash: String,
    /// Wall clock of the world's run.
    pub ms: u64,
    pub final_alive: u32,
    #[serde(default)]
    pub extinct_at: Option<u64>,
}

impl WorldRecord {
    pub fn of(run: &CellRun) -> Self {
        Self {
            cell: run.cell.clone(),
            seed: run.seed,
            ticks: run.ticks,
            dynamics_hash: format!("{:#018x}", run.dynamics_hash),
            state_hash: format!("{:#018x}", run.state_hash),
            ms: run.ms,
            final_alive: run.final_alive as u32,
            extinct_at: run.extinct_at,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub lab_result: u32,
    pub design: Design,
    #[serde(default)]
    pub selection: Selection,
    #[serde(default)]
    pub environment: Environment,
    /// Worlds run at once.
    pub jobs: usize,
    /// Whether invariants were checked every tick.
    #[serde(default)]
    pub check: bool,
    /// ISO 8601 UTC.
    pub started: String,
    pub finished: String,
    pub wall_ms: u64,
    pub worlds: Vec<WorldRecord>,
}

impl Manifest {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a manifest serialises")
    }

    pub fn from_json(text: &str) -> Result<Manifest, Error> {
        let m: Manifest = serde_json::from_str(text)?;
        if m.lab_result != LAB_RESULT_FORMAT {
            return Err(Error(format!(
                "lab_result {} is not {LAB_RESULT_FORMAT}",
                m.lab_result
            )));
        }
        Ok(m)
    }

    /// Sum of the worlds' ticks.
    pub fn world_ticks(&self) -> u64 {
        self.worlds.iter().map(|w| w.ticks).sum()
    }
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Seconds since the Unix epoch as `YYYY-MM-DDThh:mm:ssZ`.
pub fn iso8601_utc(unix_secs: u64) -> String {
    let (y, m, d) = civil_from_days((unix_secs / 86_400) as i64);
    let rem = unix_secs % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

/// Seconds since the Unix epoch now (0 without a clock: wasm).
pub fn unix_now() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }
    #[cfg(target_arch = "wasm32")]
    {
        0
    }
}

/// The current time as `YYYY-MM-DDThh:mm:ssZ`.
pub fn now_iso8601() -> String {
    iso8601_utc(unix_now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_matches_known_instants() {
        assert_eq!(iso8601_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601_utc(951_868_800), "2000-03-01T00:00:00Z");
        assert_eq!(iso8601_utc(1_709_251_199), "2024-02-29T23:59:59Z");
        // EXP-004's RUN.log: date: 2026-10-18T01:35:33Z.
        assert_eq!(iso8601_utc(1_792_287_333), "2026-10-18T01:35:33Z");
    }
}
