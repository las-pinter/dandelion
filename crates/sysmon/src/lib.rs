//! System metrics collection and short in-memory history.
//!
//! [`Collector`] reads the machine's counters from `/proc` and `/sys` and
//! turns them into a [`Snapshot`]. Rates are derived from the delta between
//! two samples, so the first sample reports zero rates.
//!
//! [`History`] keeps a compact second-by-second series for one hour and
//! per-minute averages and maxima for one week, cheaply queryable by time
//! range. Everything it exposes is serialisable for later persistence.
//!
//! The crate reads Linux procfs directly instead of depending on a generic
//! system-information crate: it needs the `Cached` memory split that such
//! crates do not expose, it must control exactly which counters are read on
//! every tick, and it needs a delta model that can be tested without the
//! real clock.
//!
//! ```no_run
//! use dandelion_sysmon::{Collector, History};
//!
//! # async fn example() {
//! let mut history = History::new();
//! let mut collector = Collector::new();
//! let snapshot = collector.sample().expect("read /proc");
//! history.ingest(&snapshot);
//! # }
//! ```

mod collector;
mod error;
mod history;
mod procfs;
mod rates;
mod types;

pub use collector::Collector;
pub use error::Error;
pub use history::{
    History, HistorySnapshot, MINUTES_PER_WEEK, MetricsPoint, MinuteAggregate, SECONDS_PER_HOUR,
    Stat,
};
pub use types::{
    Cpu, DiskMount, LoadAverage, Memory, NetworkInterface, Process, Snapshot, Temperature,
};
