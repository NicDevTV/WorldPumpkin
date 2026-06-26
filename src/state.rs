// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use crate::{
    config::Config,
    engine::{BlockPos, HistoryEntry, Selection},
};
use std::collections::{HashMap, VecDeque};

pub struct PluginState {
    config: Config,
    sessions: HashMap<String, PlayerSession>,
}

impl PluginState {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            sessions: HashMap::new(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn replace_config(&mut self, config: Config) {
        self.config = config;
        self.prune_all_history();
    }

    pub fn set_position(&mut self, owner: String, slot: SelectionSlot, pos: BlockPos) -> Selection {
        let session = self.sessions.entry(owner).or_default();
        match slot {
            SelectionSlot::Pos1 => session.selection.pos1 = Some(pos),
            SelectionSlot::Pos2 => session.selection.pos2 = Some(pos),
        }
        session.selection
    }

    pub fn selection(&self, owner: &str) -> Option<Selection> {
        self.sessions.get(owner).map(|session| session.selection)
    }

    pub fn push_history(&mut self, owner: String, entry: HistoryEntry) {
        let max_entries = self.config.max_history_entries;
        let max_blocks = self.config.max_history_blocks;
        let session = self.sessions.entry(owner).or_default();

        session.history.push_front(entry);
        while session.history.len() > max_entries {
            session.history.pop_back();
        }
        while total_history_blocks(&session.history) > max_blocks {
            session.history.pop_back();
        }
    }

    pub fn pop_history(&mut self, owner: &str) -> Option<HistoryEntry> {
        self.sessions
            .get_mut(owner)
            .and_then(|session| session.history.pop_front())
    }

    fn prune_all_history(&mut self) {
        let max_entries = self.config.max_history_entries;
        let max_blocks = self.config.max_history_blocks;
        for session in self.sessions.values_mut() {
            while session.history.len() > max_entries {
                session.history.pop_back();
            }
            while total_history_blocks(&session.history) > max_blocks {
                session.history.pop_back();
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum SelectionSlot {
    Pos1,
    Pos2,
}

#[derive(Default)]
struct PlayerSession {
    selection: Selection,
    history: VecDeque<HistoryEntry>,
}

fn total_history_blocks(history: &VecDeque<HistoryEntry>) -> usize {
    history.iter().map(HistoryEntry::len).sum()
}
