// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 zickrian
// Copyright (C) 2026 Galyarder Labs

//! "Remind me in ten minutes": reminders kept until shown, so one due while the
//! screen was locked still shows afterwards.

use serde::{Deserialize, Serialize};

/// The most reminders kept at once.
pub const CAPACITY: usize = 50;
/// How long a shown reminder stays in the history, so nothing quietly vanishes.
pub const HISTORY_MS: i64 = 7 * 24 * 3600 * 1000;
/// The most history entries kept at once.
pub const HISTORY_CAPACITY: usize = 200;

/// One reminder: what to say and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reminder {
    pub id: u64,
    pub text: String,
    /// When it is due, in milliseconds since the Unix epoch.
    pub due_ms: i64,
}

/// A reminder that was shown, kept for the history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PastReminder {
    pub id: u64,
    pub text: String,
    /// When it was due, in milliseconds since the Unix epoch.
    pub due_ms: i64,
    /// When it was shown, in milliseconds since the Unix epoch.
    pub shown_at_ms: i64,
}

/// The pending reminders, kept sorted by due time, with ids that never repeat; and the
/// recently shown ones, newest first, so a reminder due while Notchfolk was off can still
/// be looked up after it fired at login.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Reminders {
    items: Vec<Reminder>,
    next_id: u64,
    #[serde(default)]
    history: Vec<PastReminder>,
}

impl Reminders {
    /// By due time, soonest first.
    pub fn items(&self) -> &[Reminder] {
        &self.items
    }

    /// Adds a reminder and returns its id; `None` when blank or full.
    pub fn add(&mut self, text: &str, due_ms: i64) -> Option<u64> {
        let text = text.trim();
        if text.is_empty() || self.items.len() >= CAPACITY {
            return None;
        }
        self.next_id += 1;
        let at = self.items.partition_point(|item| item.due_ms <= due_ms);
        self.items.insert(
            at,
            Reminder {
                id: self.next_id,
                text: text.to_owned(),
                due_ms,
            },
        );
        Some(self.next_id)
    }

    /// The recently shown reminders, newest first.
    pub fn history(&self) -> &[PastReminder] {
        &self.history
    }

    /// Removes a shown reminder into the history; returns whether it was there.
    pub fn dismiss(&mut self, id: u64, shown_at_ms: i64) -> bool {
        let Some(at) = self.items.iter().position(|item| item.id == id) else {
            return false;
        };
        let reminder = self.items.remove(at);
        self.history.insert(
            0,
            PastReminder {
                id: reminder.id,
                text: reminder.text,
                due_ms: reminder.due_ms,
                shown_at_ms,
            },
        );
        self.prune_history(shown_at_ms);
        true
    }

    /// Drops history older than `HISTORY_MS` and keeps the list within its capacity.
    pub fn prune_history(&mut self, now_ms: i64) {
        self.history
            .retain(|past| now_ms.saturating_sub(past.shown_at_ms) < HISTORY_MS);
        self.history.truncate(HISTORY_CAPACITY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_reminders_soonest_first() {
        let mut reminders = Reminders::default();
        let late = reminders.add("Call mum", 3000).unwrap();
        let soon = reminders.add("Tea", 1000).unwrap();
        let ids: Vec<u64> = reminders.items().iter().map(|r| r.id).collect();
        assert_eq!(ids, [soon, late]);
    }

    #[test]
    fn refuses_blank_text_and_too_many() {
        let mut reminders = Reminders::default();
        assert_eq!(reminders.add("  ", 1000), None);
        for n in 0..CAPACITY {
            assert!(reminders.add("x", n as i64).is_some());
        }
        assert_eq!(reminders.add("one more", 0), None);
    }

    #[test]
    fn dismisses_by_id_into_the_history() {
        let mut reminders = Reminders::default();
        let id = reminders.add("Tea", 1000).unwrap();
        assert!(reminders.dismiss(id, 5000));
        assert!(!reminders.dismiss(id, 5001));
        assert!(reminders.items().is_empty());
        assert_eq!(reminders.history().len(), 1);
        assert_eq!(reminders.history()[0].text, "Tea");
        assert_eq!(reminders.history()[0].due_ms, 1000);
        assert_eq!(reminders.history()[0].shown_at_ms, 5000);
    }

    #[test]
    fn the_history_is_newest_first_and_prunes() {
        let mut reminders = Reminders::default();
        let first = reminders.add("First", 1000).unwrap();
        let second = reminders.add("Second", 2000).unwrap();
        reminders.dismiss(first, 10_000);
        reminders.dismiss(second, 20_000);
        let texts: Vec<&str> = reminders
            .history()
            .iter()
            .map(|p| p.text.as_str())
            .collect();
        assert_eq!(texts, ["Second", "First"]);
        reminders.prune_history(10_000 + HISTORY_MS);
        let texts: Vec<&str> = reminders
            .history()
            .iter()
            .map(|p| p.text.as_str())
            .collect();
        assert_eq!(texts, ["Second"]);
    }

    #[test]
    fn an_old_store_without_a_history_loads() {
        let json = r#"{"items":[],"next_id":3}"#;
        let reminders: Reminders = serde_json::from_str(json).unwrap();
        assert!(reminders.history().is_empty());
    }
}
