use crate::ipc::messages::{timestamp_ms, HelperMessage};
use crate::{logging, paths, storage};
use anyhow::{Context, Result};
use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

const USAGE_SCHEMA_VERSION: u32 = 1;
const RETAIN_DAYS: usize = 400;

#[derive(Clone)]
pub struct UsageTracker {
    inner: Arc<Mutex<UsageInner>>,
    event_tx: broadcast::Sender<HelperMessage>,
}

struct UsageInner {
    stats: UsageStats,
    path: PathBuf,
    dirty: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageStats {
    schema_version: u32,
    total_actions: u64,
    first_recorded_at: u64,
    last_recorded_at: u64,
    by_action: BTreeMap<String, u64>,
    days: Vec<UsageDay>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageDay {
    date: String,
    actions: u64,
    by_action: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub today: String,
    pub today_actions: u64,
    pub total_actions: u64,
    pub first_recorded_at: u64,
    pub last_recorded_at: u64,
    pub today_by_action: BTreeMap<String, u64>,
}

impl Default for UsageStats {
    fn default() -> Self {
        Self {
            schema_version: USAGE_SCHEMA_VERSION,
            total_actions: 0,
            first_recorded_at: 0,
            last_recorded_at: 0,
            by_action: BTreeMap::new(),
            days: Vec::new(),
        }
    }
}

impl UsageStats {
    fn normalized(mut self) -> Self {
        self.schema_version = USAGE_SCHEMA_VERSION;
        self.days.sort_by(|left, right| left.date.cmp(&right.date));
        if self.days.len() > RETAIN_DAYS {
            self.days.drain(0..self.days.len() - RETAIN_DAYS);
        }
        self
    }

    fn record(&mut self, date: &str, action: &str, now_ms: u64) {
        let action = normalize_action_key(action);
        self.total_actions = self.total_actions.saturating_add(1);
        *self.by_action.entry(action.clone()).or_default() += 1;
        if self.first_recorded_at == 0 {
            self.first_recorded_at = now_ms;
        }
        self.last_recorded_at = now_ms;

        let day = match self
            .days
            .binary_search_by(|day| day.date.as_str().cmp(date))
        {
            Ok(index) => &mut self.days[index],
            Err(index) => {
                self.days.insert(
                    index,
                    UsageDay {
                        date: date.to_string(),
                        actions: 0,
                        by_action: BTreeMap::new(),
                    },
                );
                &mut self.days[index]
            }
        };
        day.actions = day.actions.saturating_add(1);
        *day.by_action.entry(action).or_default() += 1;

        if self.days.len() > RETAIN_DAYS {
            self.days.drain(0..self.days.len() - RETAIN_DAYS);
        }
    }

    fn snapshot(&self, today: &str) -> UsageSnapshot {
        let day = self.days.iter().find(|day| day.date == today);
        UsageSnapshot {
            today: today.to_string(),
            today_actions: day.map_or(0, |day| day.actions),
            total_actions: self.total_actions,
            first_recorded_at: self.first_recorded_at,
            last_recorded_at: self.last_recorded_at,
            today_by_action: day.map(|day| day.by_action.clone()).unwrap_or_default(),
        }
    }
}

impl UsageTracker {
    pub fn load(event_tx: broadcast::Sender<HelperMessage>) -> Result<Self> {
        let path =
            paths::data_file("usage-stats.json").context("helper data directory is unavailable")?;
        let stats = match storage::read_json_with_backup::<UsageStats>(&path) {
            Ok(Some((stats, recovered))) => {
                if recovered {
                    logging::write_line("usage: recovered statistics from backup");
                }
                stats.normalized()
            }
            Ok(None) => UsageStats::default(),
            Err(error) => {
                logging::write_line(format!("usage: could not load statistics: {error:#}"));
                UsageStats::default()
            }
        };
        Ok(Self {
            inner: Arc::new(Mutex::new(UsageInner {
                stats,
                path,
                dirty: false,
            })),
            event_tx,
        })
    }

    pub fn snapshot(&self) -> UsageSnapshot {
        let today = local_date();
        self.inner
            .lock()
            .map(|inner| inner.stats.snapshot(&today))
            .unwrap_or_else(|_| UsageStats::default().snapshot(&today))
    }

    pub fn message(&self) -> HelperMessage {
        HelperMessage::new("usage.status", json!(self.snapshot()))
    }

    pub async fn run(
        &self,
        mut event_rx: broadcast::Receiver<HelperMessage>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) {
        let mut flush_interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            tokio::select! {
                event = event_rx.recv() => {
                    match event {
                        Ok(message) if message.kind == "action.triggered" => {
                            self.record(action_key(&message));
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            logging::write_line(format!("usage: event stream lagged by {skipped}"));
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = flush_interval.tick() => {
                    if let Err(error) = self.flush_if_dirty() {
                        logging::write_line(format!("usage: save failed: {error:#}"));
                    }
                }
                _ = shutdown_rx.recv() => {
                    if let Err(error) = self.flush_if_dirty() {
                        logging::write_line(format!("usage: final save failed: {error:#}"));
                    }
                    break;
                }
            }
        }
    }

    fn record(&self, action: String) {
        let today = local_date();
        let now_ms = timestamp_ms();
        let snapshot = match self.inner.lock() {
            Ok(mut inner) => {
                inner.stats.record(&today, &action, now_ms);
                inner.dirty = true;
                inner.stats.snapshot(&today)
            }
            Err(_) => return,
        };
        let _ = self
            .event_tx
            .send(HelperMessage::new("usage.status", json!(snapshot)));
    }

    fn flush_if_dirty(&self) -> Result<()> {
        let (path, stats) = {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| anyhow::anyhow!("usage lock is poisoned"))?;
            if !inner.dirty {
                return Ok(());
            }
            inner.dirty = false;
            (inner.path.clone(), inner.stats.clone())
        };

        if let Err(error) = storage::write_json_with_backup(&path, &stats) {
            if let Ok(mut inner) = self.inner.lock() {
                inner.dirty = true;
            }
            return Err(error).with_context(|| format!("write {}", path.display()));
        }
        Ok(())
    }
}

fn local_date() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn action_key(message: &HelperMessage) -> String {
    message
        .data
        .get("kind")
        .and_then(|value| value.as_str())
        .or_else(|| message.data.get("source").and_then(|value| value.as_str()))
        .unwrap_or("unknown")
        .to_string()
}

fn normalize_action_key(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized.len() > 64 {
        "unknown".to_string()
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_daily_totals_and_action_breakdown() {
        let mut stats = UsageStats::default();
        stats.record("2026-07-14", "show-desktop", 100);
        stats.record("2026-07-14", "show-desktop", 200);
        stats.record("2026-07-14", "edge-hide.collapse", 300);

        let snapshot = stats.snapshot("2026-07-14");
        assert_eq!(snapshot.today_actions, 3);
        assert_eq!(snapshot.total_actions, 3);
        assert_eq!(snapshot.today_by_action.get("show-desktop"), Some(&2));
        assert_eq!(snapshot.first_recorded_at, 100);
        assert_eq!(snapshot.last_recorded_at, 300);
    }

    #[test]
    fn new_day_resets_today_without_resetting_lifetime_total() {
        let mut stats = UsageStats::default();
        stats.record("2026-07-14", "shortcut", 100);
        stats.record("2026-07-15", "shortcut", 200);

        let snapshot = stats.snapshot("2026-07-15");
        assert_eq!(snapshot.today_actions, 1);
        assert_eq!(snapshot.total_actions, 2);
    }

    #[test]
    fn history_is_bounded_but_lifetime_total_is_preserved() {
        let mut stats = UsageStats::default();
        for day in 0..(RETAIN_DAYS + 5) {
            stats.record(&format!("2026-{day:04}"), "shortcut", day as u64);
        }

        assert_eq!(stats.days.len(), RETAIN_DAYS);
        assert_eq!(stats.total_actions, (RETAIN_DAYS + 5) as u64);
    }
}
