//! Experiment designs (ADR-0015 D2): the JSON a human writes for one
//! experiment, and the rules that turn a (design, cell, seed) into a
//! `World`. The rules are `cz run`'s `build_world`, moved here so that the
//! CLI, `cz lab` and the browser's `cz-lab-web` build the same world from
//! the same words.
//!
//! ```json
//! {
//!   "format": 1,
//!   "name": "EXP-004",
//!   "base": { "policy": "institution", "width": 128, "height": 128,
//!             "agents": 1000, "patches": 23,
//!             "config": { "enforcer_motions": false } },
//!   "ticks": 20000, "window_pct": 50, "sample": 100,
//!   "seeds": { "start": 0, "count": 30 },
//!   "cells": [
//!     { "name": "le0" },
//!     { "name": "le", "schedule": [ { "tick": 0, "intervention":
//!         { "op": "enact", "norm": { "kind": "encroach", "standing_ticks": 32,
//!                                    "sanction": 1, "except_sanction": false } } } ] },
//!     { "name": "l0", "policy": "learner", "preset": "nomos",
//!       "config": { "learner_states": 128 } },
//!     { "name": "n_rand", "policy": "random", "preset": "nomos",
//!       "seeds": { "start": 0, "count": 10 }, "sample": 10, "schedule": [ "…" ] }
//!   ],
//!   "contrasts": [
//!     { "name": "LE − LE0", "treat": "le", "control": "le0",
//!       "metric": { "ratio": { "num": "encroachments_learner", "den": "gathers_learner" } },
//!       "window": [10000, 20000] }
//!   ]
//! }
//! ```
//!
//! **Precedence** of a cell's `WorldConfig`: the preset (`base.preset`,
//! overridden by `cell.preset`; without one, the institution mix starts
//! from `WorldConfig::institution_preset()` and every other mix from
//! `WorldConfig::default()`), then what the policy mix implies (the
//! institution mix keeps `institution_preset`'s 128 rows and enforcers'
//! share whatever the preset; `reflex` and `mixed-learning` switch
//! mutation on — ADR-0013 D6), then `base.width / height / agents /
//! patches`, then `base.config`, then `cell.config` (both are partial
//! `WorldConfig`s in the kernel's JSON form, merged key by key — nested
//! objects such as `perception` and `charter` merge field by field, `null`
//! clears an `Option`, an unknown key is an error), and the run's seed
//! last (`seed` may not appear in a config override). This is exactly
//! `cz sweep`'s rule "every flag defaults to the base config's value; a
//! flag given explicitly overrides it".

use crate::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use simulation_core::{Intervention, PolicyKind, World, WorldConfig};

/// `"format"` of a design file. Bumped when the schema changes
/// incompatibly; older files are rejected.
pub const DESIGN_FORMAT: u32 = 1;

/// The founding mixes `cz run --policy` knows (ADR-0013 D6, ADR-0014 D8).
pub const POLICY_MIXES: [&str; 7] = [
    "forager",
    "random",
    "mixed",
    "reflex",
    "learner",
    "mixed-learning",
    "institution",
];

/// The configs a design may start from (`WorldConfig::default()`,
/// `nomos_preset()`, `institution_preset()`).
pub const PRESETS: [&str; 3] = ["default", "nomos", "institution"];

fn default_policy() -> String {
    "forager".to_string()
}

fn default_window_pct() -> u64 {
    50
}

fn default_sample() -> u64 {
    100
}

/// One experiment: the base world, the run, the cells and the contrasts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Design {
    /// Always [`DESIGN_FORMAT`].
    pub format: u32,
    /// The experiment's name (`EXP-004`); the manifest and the summary
    /// carry it.
    pub name: String,
    /// Free text: where the specification lives, what the cells stand for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub base: Base,
    /// Ticks every world runs.
    pub ticks: u64,
    /// The analysis window: the last `window_pct` percent of `ticks`
    /// (`cz sweep --window`; 50 by default).
    #[serde(default = "default_window_pct")]
    pub window_pct: u64,
    /// Ticks between `series` rows (`cz sweep --sample`; 100 by default).
    #[serde(default = "default_sample")]
    pub sample: u64,
    /// The seeds every cell runs unless the cell says otherwise.
    pub seeds: Seeds,
    pub cells: Vec<Cell>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contrasts: Vec<Contrast>,
}

/// The world every cell starts from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Base {
    /// A founding mix of [`POLICY_MIXES`] (`forager` by default).
    #[serde(default = "default_policy")]
    pub policy: String,
    /// One of [`PRESETS`]; without one, the institution mix starts from
    /// `institution_preset()` and the others from `default()`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u16>,
    /// Founders (`WorldConfig.initial_agents`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents: Option<u32>,
    /// Resource patches (`WorldConfig.patch_count`). `cz sweep` passes
    /// `side² / 700` (23 at 128²); the kernel's default is 24.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patches: Option<u32>,
    /// A partial `WorldConfig` (the kernel's JSON field names) laid over
    /// the preset; `null` or absent for none.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub config: Value,
}

impl Default for Base {
    fn default() -> Self {
        Self {
            policy: default_policy(),
            preset: None,
            width: None,
            height: None,
            agents: None,
            patches: None,
            config: Value::Null,
        }
    }
}

/// A range of seeds: `start, start + 1, …, start + count − 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seeds {
    pub start: u64,
    pub count: u64,
}

impl Seeds {
    /// One past the last seed.
    pub fn end(self) -> u64 {
        self.start + self.count
    }

    pub fn iter(self) -> std::ops::Range<u64> {
        self.start..self.end()
    }

    pub fn contains(self, seed: u64) -> bool {
        self.iter().contains(&seed)
    }
}

/// One condition: the base plus overrides, a schedule of interventions,
/// and optionally its own seeds and sampling interval (EXP-004's null
/// models run 10 seeds where the main arms run 30; its random arm samples
/// every 10 ticks because it is extinct by tick 2,000).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    /// File-name safe (`[A-Za-z0-9_-]`): the CSVs are `<name>_summary.csv`
    /// and so on.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub config: Value,
    /// Interventions applied between ticks, at `tick` (before that tick
    /// runs), in this order within a tick.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schedule: Vec<Scheduled>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seeds: Option<Seeds>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<u64>,
}

/// An intervention and the tick it is applied at (`World::intervene`
/// right before that tick's `step`, as `cz sweep --enact-at` did).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scheduled {
    pub tick: u64,
    pub intervention: Intervention,
}

/// One or several CSV columns, summed. Written as a string when it is one
/// column, as an array otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Columns(pub Vec<String>);

impl Serialize for Columns {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.0.len() == 1 {
            serializer.serialize_str(&self.0[0])
        } else {
            self.0.serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for Columns {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            One(String),
            Many(Vec<String>),
        }
        Ok(match Raw::deserialize(deserializer)? {
            Raw::One(s) => Columns(vec![s]),
            Raw::Many(v) => Columns(v),
        })
    }
}

impl std::fmt::Display for Columns {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.join("+"))
    }
}

/// What a contrast reads from one seed's rows (EXP-004's `summarize.py`
/// readings, generalised).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Σ `num` / Σ `den` over the `series` rows in the window (`E_L` =
    /// `encroachments_learner / gathers_learner`, `S′` = `sanctions /
    /// violations`, the share of violent deaths = `deaths_violence /
    /// (deaths_violence + deaths_starvation + deaths_old_age)`). `None`
    /// when the denominator is 0.
    Ratio { num: Columns, den: Columns },
    /// Mean of a `series` column over the window's rows (`alive_learner`).
    Mean(String),
    /// Σ of a `series` column over the window / the window's ticks.
    PerTick(String),
    /// A `summary` column as `cz sweep` wrote it (`win_act_rest`,
    /// `win_consumed_per_agent_tick`); the window is the sweep's.
    Summary(String),
    /// Σ `num` / Σ `den` over the seed's `summary` columns
    /// (`win_deaths_violence / (win_deaths_violence + win_deaths_starvation
    /// + win_deaths_old_age)`).
    SummaryRatio { num: Columns, den: Columns },
}

impl std::fmt::Display for Metric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Metric::Ratio { num, den } => write!(f, "ratio({num}/{den})"),
            Metric::Mean(c) => write!(f, "mean({c})"),
            Metric::PerTick(c) => write!(f, "per_tick({c})"),
            Metric::Summary(c) => write!(f, "summary({c})"),
            Metric::SummaryRatio { num, den } => write!(f, "summary_ratio({num}/{den})"),
        }
    }
}

impl Metric {
    /// Whether the metric reads the `series` rows of a window (true) or
    /// the `summary` row (false).
    pub fn reads_series(&self) -> bool {
        matches!(
            self,
            Metric::Ratio { .. } | Metric::Mean(_) | Metric::PerTick(_)
        )
    }
}

/// Two cells whose same-seed differences (`treat − control`) are reported.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contrast {
    pub name: String,
    pub treat: String,
    pub control: String,
    pub metric: Metric,
    /// `(from, to]`: the `series` rows with `from < tick <= to`. Without
    /// one, the design's analysis window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<(u64, u64)>,
}

/// The founding mix a name stands for: the rule `cz run --policy` and
/// cz-web's `policy_mix` share.
pub fn policy_mix(name: &str) -> Result<fn(u32) -> PolicyKind, Error> {
    match name {
        "forager" => Ok(|_| PolicyKind::Forager),
        "random" => Ok(|_| PolicyKind::Random),
        "mixed" => Ok(|i| {
            if i % 2 == 0 {
                PolicyKind::Forager
            } else {
                PolicyKind::Random
            }
        }),
        "reflex" => Ok(|_| PolicyKind::Reflex),
        "learner" => Ok(|_| PolicyKind::Learner),
        "mixed-learning" => Ok(|i| match i % 3 {
            0 => PolicyKind::Forager,
            1 => PolicyKind::Reflex,
            _ => PolicyKind::Learner,
        }),
        // Learners with every tenth founder an enforcer (ADR-0014 D8).
        "institution" => Ok(|i| {
            if i % 10 == 0 {
                PolicyKind::Enforcer
            } else {
                PolicyKind::Learner
            }
        }),
        other => Err(Error(format!(
            "unknown policy mix {other:?}: expected one of {}",
            POLICY_MIXES.join(", ")
        ))),
    }
}

/// Whether a founding mix turns weight mutation on by default (ADR-0013
/// D6: the reflex policy evolves between generations).
pub fn evolves(name: &str) -> bool {
    matches!(name, "reflex" | "mixed-learning")
}

/// The config a cell starts from, before any override: the named preset,
/// or — without one — `institution_preset()` for the institution mix and
/// `default()` otherwise; then what the mix implies (`cz run`'s rule).
pub fn preset_config(preset: Option<&str>, policy: &str) -> Result<WorldConfig, Error> {
    let institution = policy == "institution";
    let mut cfg = match preset {
        None if institution => WorldConfig::institution_preset(),
        None | Some("default") => WorldConfig::default(),
        Some("nomos") => WorldConfig::nomos_preset(),
        Some("institution") => WorldConfig::institution_preset(),
        Some(other) => {
            return Err(Error(format!(
                "unknown preset {other:?}: expected one of {}",
                PRESETS.join(", ")
            )))
        }
    };
    if institution {
        // `--preset nomos --policy institution` is the institution world
        // too: 128 rows and the enforcers' share (ADR-0014 D11, 4-9).
        let p = WorldConfig::institution_preset();
        cfg.learner_states = p.learner_states;
        cfg.enforcer_share_permille = p.enforcer_share_permille;
    }
    cfg.mutation_enabled = evolves(policy);
    Ok(cfg)
}

/// Lays a partial config (`over`, a JSON object) over a full one
/// (`target`, the kernel's JSON form of a `WorldConfig`): objects merge
/// field by field, anything else replaces, an unknown key or a `seed` is
/// an error. `null` as the whole override means "nothing".
pub fn merge_config(target: &mut Value, over: &Value, path: &str) -> Result<(), Error> {
    if over.is_null() {
        return Ok(());
    }
    let Value::Object(over) = over else {
        return Err(Error(format!(
            "{path}: a config override must be a JSON object"
        )));
    };
    let Value::Object(target) = target else {
        return Err(Error(format!("{path}: cannot merge into a non-object")));
    };
    for (key, value) in over {
        let here = format!("{path}.{key}");
        let Some(slot) = target.get_mut(key) else {
            return Err(Error(format!("{here}: not a WorldConfig field")));
        };
        match (slot, value) {
            (slot @ Value::Object(_), Value::Object(_)) => merge_config(slot, value, &here)?,
            (slot, value) => *slot = value.clone(),
        }
    }
    Ok(())
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

impl Design {
    /// Parses and validates a design file.
    pub fn from_json(text: &str) -> Result<Design, Error> {
        let design: Design = serde_json::from_str(text)?;
        design.validate()?;
        Ok(design)
    }

    /// The design as pretty JSON (what `design.json` holds).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a design serialises")
    }

    /// Everything a run needs to be well-defined: the format, the run
    /// lengths, unique file-safe cell names, schedules inside the run,
    /// configs that build (each cell's config is built for its first
    /// seed), contrasts over existing distinct cells with windows inside
    /// the run.
    pub fn validate(&self) -> Result<(), Error> {
        if self.format != DESIGN_FORMAT {
            return Err(Error(format!(
                "design format {} is not {DESIGN_FORMAT}",
                self.format
            )));
        }
        if self.name.is_empty() {
            return Err("design name is empty".into());
        }
        if self.ticks == 0 {
            return Err("ticks must be > 0".into());
        }
        if self.window_pct > 100 {
            return Err("window_pct must be <= 100".into());
        }
        if self.sample == 0 {
            return Err("sample must be > 0".into());
        }
        if self.seeds.count == 0 {
            return Err("seeds.count must be > 0".into());
        }
        if self.cells.is_empty() {
            return Err("a design needs at least one cell".into());
        }
        for (i, cell) in self.cells.iter().enumerate() {
            if !valid_name(&cell.name) {
                return Err(Error(format!(
                    "cells[{i}]: name {:?} must be [A-Za-z0-9_-]+",
                    cell.name
                )));
            }
            if self.cells[..i].iter().any(|c| c.name == cell.name) {
                return Err(Error(format!("cells[{i}]: duplicate name {:?}", cell.name)));
            }
            if cell.sample == Some(0) {
                return Err(Error(format!("cell {:?}: sample must be > 0", cell.name)));
            }
            if cell.seeds.is_some_and(|s| s.count == 0) {
                return Err(Error(format!(
                    "cell {:?}: seeds.count must be > 0",
                    cell.name
                )));
            }
            for s in &cell.schedule {
                if s.tick >= self.ticks {
                    return Err(Error(format!(
                        "cell {:?}: a scheduled tick {} is not before the run's end {}",
                        cell.name, s.tick, self.ticks
                    )));
                }
            }
            policy_mix(self.policy_of(cell))?;
            self.world_config(cell, self.seeds_of(cell).start)?;
        }
        for (i, c) in self.contrasts.iter().enumerate() {
            for (role, name) in [("treat", &c.treat), ("control", &c.control)] {
                if self.cell(name).is_none() {
                    return Err(Error(format!(
                        "contrasts[{i}] {:?}: {role} {name:?} is not a cell",
                        c.name
                    )));
                }
            }
            if c.treat == c.control {
                return Err(Error(format!(
                    "contrasts[{i}] {:?}: treat and control are the same cell",
                    c.name
                )));
            }
            if let Some((from, to)) = c.window {
                if from >= to || to > self.ticks {
                    return Err(Error(format!(
                        "contrasts[{i}] {:?}: window ({from}, {to}] must lie within 0..={}",
                        c.name, self.ticks
                    )));
                }
            }
            let empty = match &c.metric {
                Metric::Ratio { num, den } | Metric::SummaryRatio { num, den } => {
                    num.0.is_empty() || den.0.is_empty()
                }
                Metric::Mean(col) | Metric::PerTick(col) | Metric::Summary(col) => col.is_empty(),
            };
            if empty {
                return Err(Error(format!(
                    "contrasts[{i}] {:?}: the metric names no column",
                    c.name
                )));
            }
        }
        Ok(())
    }

    pub fn cell(&self, name: &str) -> Option<&Cell> {
        self.cells.iter().find(|c| c.name == name)
    }

    /// The cell's founding mix: its own or the base's.
    pub fn policy_of<'a>(&'a self, cell: &'a Cell) -> &'a str {
        cell.policy.as_deref().unwrap_or(&self.base.policy)
    }

    /// The cell's preset: its own, the base's, or none.
    pub fn preset_of<'a>(&'a self, cell: &'a Cell) -> Option<&'a str> {
        cell.preset.as_deref().or(self.base.preset.as_deref())
    }

    pub fn seeds_of(&self, cell: &Cell) -> Seeds {
        cell.seeds.unwrap_or(self.seeds)
    }

    pub fn sample_of(&self, cell: &Cell) -> u64 {
        cell.sample.unwrap_or(self.sample).max(1)
    }

    /// First tick of the analysis window: the summary covers the ticks
    /// `window_start < tick <= ticks` (as `cz sweep` always did).
    pub fn window_start(&self) -> u64 {
        self.ticks - self.ticks * self.window_pct.min(100) / 100
    }

    /// The analysis window as a contrast's `(from, to]`.
    pub fn window(&self) -> (u64, u64) {
        (self.window_start(), self.ticks)
    }

    /// The `WorldConfig` of one world, by the precedence in the module
    /// docs.
    pub fn world_config(&self, cell: &Cell, seed: u64) -> Result<WorldConfig, Error> {
        let policy = self.policy_of(cell);
        let mut cfg = preset_config(self.preset_of(cell), policy)?;
        cfg.seed = seed;
        if let Some(w) = self.base.width {
            cfg.width = w;
        }
        if let Some(h) = self.base.height {
            cfg.height = h;
        }
        if let Some(a) = self.base.agents {
            cfg.initial_agents = a;
        }
        if let Some(p) = self.base.patches {
            cfg.patch_count = p;
        }
        let mut json = serde_json::to_value(&cfg)?;
        for (over, path) in [
            (&self.base.config, "base.config".to_string()),
            (&cell.config, format!("cell {:?} config", cell.name)),
        ] {
            if let Value::Object(map) = over {
                if map.contains_key("seed") {
                    return Err(Error(format!("{path}.seed: the seed is the run's")));
                }
            }
            merge_config(&mut json, over, &path)?;
        }
        let cfg: WorldConfig = serde_json::from_value(json)
            .map_err(|e| Error(format!("cell {:?}: config does not parse: {e}", cell.name)))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// One world of the design, ready to step (tick 0, nothing applied).
    pub fn build_world(&self, cell: &Cell, seed: u64) -> Result<World, Error> {
        let cfg = self.world_config(cell, seed)?;
        let mix = policy_mix(self.policy_of(cell))?;
        Ok(World::generate(cfg, mix)?)
    }

    /// The worlds a run covers, as `(cell index, seed)` in cell order and
    /// ascending seed: all of them, or those of the named cells and / or
    /// the seeds in `[start, end)`.
    pub fn jobs(
        &self,
        cells: Option<&[String]>,
        seeds: Option<(u64, u64)>,
    ) -> Result<Vec<(usize, u64)>, Error> {
        if let Some(names) = cells {
            for n in names {
                if self.cell(n).is_none() {
                    return Err(Error(format!("no cell named {n:?}")));
                }
            }
        }
        let mut out = Vec::new();
        for (i, cell) in self.cells.iter().enumerate() {
            if cells.is_some_and(|names| !names.contains(&cell.name)) {
                continue;
            }
            for seed in self.seeds_of(cell).iter() {
                if seeds.is_some_and(|(start, end)| seed < start || seed >= end) {
                    continue;
                }
                out.push((i, seed));
            }
        }
        Ok(out)
    }
}
