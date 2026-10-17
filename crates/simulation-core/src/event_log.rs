//! Append-only event ledger (ADR-0010, task 2a-2).
//!
//! A ledger is the sequence of lifecycle events and interventions of one
//! world in nondecreasing tick order. Together with a snapshot it is what
//! replay (2a-4) and the Toy 1 timeline are built on. The kernel never opens
//! files: the JSON Lines helpers take `Write` / `BufRead`, so the CLI, a test
//! or (later) the browser decides where the bytes go.
//!
//! File format (`.jsonl`): the first line is a header
//! `{"format":1,"kernel":"simulation-core 0.1.0"}`; every following line is
//! one `Event` exactly as serde renders it (see `event.rs`).

use crate::event::Event;
use crate::world::TickReport;
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};
use std::ops::Range;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventLog {
    events: Vec<Event>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogError {
    /// An event's tick is smaller than the last recorded one.
    OutOfOrder {
        tick: u64,
        last: u64,
    },
    Header(String),
    Line {
        number: usize,
        message: String,
    },
    Io(String),
}

impl std::fmt::Display for LogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogError::OutOfOrder { tick, last } => {
                write!(f, "event for tick {tick} after tick {last}")
            }
            LogError::Header(why) => write!(f, "bad ledger header: {why}"),
            LogError::Line { number, message } => write!(f, "line {number}: {message}"),
            LogError::Io(why) => write!(f, "io: {why}"),
        }
    }
}

impl std::error::Error for LogError {}

impl From<io::Error> for LogError {
    fn from(e: io::Error) -> Self {
        LogError::Io(e.to_string())
    }
}

#[derive(Serialize, Deserialize)]
struct Header {
    format: u32,
    kernel: String,
}

impl EventLog {
    pub const FORMAT: u32 = 1;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn last_tick(&self) -> Option<u64> {
        self.events.last().map(Event::tick)
    }

    /// Appends one event. Ticks must never go backwards.
    pub fn push(&mut self, e: Event) -> Result<(), LogError> {
        if let Some(last) = self.last_tick() {
            if e.tick() < last {
                return Err(LogError::OutOfOrder {
                    tick: e.tick(),
                    last,
                });
            }
        }
        self.events.push(e);
        Ok(())
    }

    /// Appends everything a tick produced, in the kernel's order.
    pub fn record(&mut self, report: &TickReport) -> Result<(), LogError> {
        for e in &report.events {
            self.push(*e)?;
        }
        Ok(())
    }

    /// Events with `ticks.start <= tick < ticks.end`, by binary search.
    pub fn range(&self, ticks: Range<u64>) -> &[Event] {
        let start = self.events.partition_point(|e| e.tick() < ticks.start);
        let end = self.events.partition_point(|e| e.tick() < ticks.end);
        &self.events[start..end.max(start)]
    }

    pub fn at(&self, tick: u64) -> &[Event] {
        self.range(tick..tick + 1)
    }

    pub fn write_jsonl(&self, w: &mut impl Write) -> io::Result<()> {
        let header = Header {
            format: Self::FORMAT,
            kernel: concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION")).to_string(),
        };
        serde_json::to_writer(&mut *w, &header)?;
        w.write_all(b"\n")?;
        for e in &self.events {
            serde_json::to_writer(&mut *w, e)?;
            w.write_all(b"\n")?;
        }
        Ok(())
    }

    pub fn read_jsonl(r: impl BufRead) -> Result<Self, LogError> {
        let mut lines = r.lines();
        let first = lines
            .next()
            .ok_or_else(|| LogError::Header("empty input".into()))??;
        let header: Header =
            serde_json::from_str(&first).map_err(|e| LogError::Header(e.to_string()))?;
        if header.format != Self::FORMAT {
            return Err(LogError::Header(format!(
                "unsupported format {} (expected {})",
                header.format,
                Self::FORMAT
            )));
        }
        let mut log = Self::new();
        for (i, line) in lines.enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let e: Event = serde_json::from_str(&line).map_err(|err| LogError::Line {
                number: i + 2,
                message: err.to_string(),
            })?;
            log.push(e)?;
        }
        Ok(log)
    }
}
