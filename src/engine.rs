// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use crate::{blocks::BlockPattern, config::Config, state::PluginState};
use pumpkin_plugin_api::{server::Server, world::World};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

mod geometry;
mod move_edit;
mod writer;

pub use geometry::{BlockPos, Cuboid, Selection};
use geometry::{CuboidIter, WallIter};
use move_edit::MoveState;
use writer::{apply_forward, replay_change, ChunkCursor, WorldAccess, WriteStrategy};

// Patterns are deterministic inside one edit, but each queued edit gets a fresh distribution.
static PATTERN_SEED: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct HistoryEntry {
    world_id: String,
    changes: Vec<BlockChange>,
}

impl HistoryEntry {
    pub fn new(world_id: String, changes: Vec<BlockChange>) -> Self {
        Self { world_id, changes }
    }

    pub fn len(&self) -> usize {
        self.changes.len()
    }

    pub fn world_id(&self) -> &str {
        &self.world_id
    }
}

#[derive(Debug)]
pub struct BlockChange {
    pos: BlockPos,
    old_state: u16,
    new_state: u16,
    old_block_entity: Option<Vec<u8>>,
    new_block_entity: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BlockSnapshot {
    state: u16,
    block_entity: Option<Vec<u8>>,
}

enum EditKind {
    Set {
        to: Arc<BlockPattern>,
    },
    Replace {
        from: u16,
        to: Arc<BlockPattern>,
    },
    Move(MoveState),
    Replay {
        history: HistoryEntry,
        direction: ReplayDirection,
        next_index: usize,
    },
}

#[derive(Clone, Copy)]
pub enum ReplayDirection {
    Undo,
    Redo,
}

pub struct EditOperation {
    owner: String,
    world: World,
    world_id: String,
    kind: EditKind,
    positions: EditPositions,
    history: Vec<BlockChange>,
    state: Arc<Mutex<PluginState>>,
    remaining: u64,
    chunk_cursor: Option<ChunkCursor>,
    pattern_seed: u64,
}

impl EditOperation {
    /// Creates a deferred cuboid fill with a fresh seed for deterministic pattern choices.
    pub fn set(
        owner: String,
        world: World,
        cuboid: Cuboid,
        to: BlockPattern,
        state: Arc<Mutex<PluginState>>,
    ) -> Self {
        let world_id = world.get_id();
        Self {
            owner,
            world,
            world_id,
            kind: EditKind::Set { to: Arc::new(to) },
            positions: EditPositions::cuboid(cuboid),
            history: Vec::new(),
            state,
            remaining: cuboid.volume(),
            chunk_cursor: None,
            pattern_seed: next_pattern_seed(),
        }
    }

    /// Creates a deferred edit that replaces only states matching `from` with pattern choices.
    pub fn replace(
        owner: String,
        world: World,
        cuboid: Cuboid,
        from: u16,
        to: BlockPattern,
        state: Arc<Mutex<PluginState>>,
    ) -> Self {
        let world_id = world.get_id();
        Self {
            owner,
            world,
            world_id,
            kind: EditKind::Replace {
                from,
                to: Arc::new(to),
            },
            positions: EditPositions::cuboid(cuboid),
            history: Vec::new(),
            state,
            remaining: cuboid.volume(),
            chunk_cursor: None,
            pattern_seed: next_pattern_seed(),
        }
    }

    /// Creates a deferred pattern fill of the cuboid's vertical faces without duplicate positions.
    pub fn walls(
        owner: String,
        world: World,
        cuboid: Cuboid,
        to: BlockPattern,
        state: Arc<Mutex<PluginState>>,
    ) -> Self {
        let world_id = world.get_id();
        let remaining = cuboid.wall_volume();
        Self {
            owner,
            world,
            world_id,
            kind: EditKind::Set { to: Arc::new(to) },
            positions: EditPositions::Walls(cuboid.walls()),
            history: Vec::new(),
            state,
            remaining,
            chunk_cursor: None,
            pattern_seed: next_pattern_seed(),
        }
    }

    pub fn move_blocks(
        owner: String,
        world: World,
        cuboid: Cuboid,
        offset: BlockPos,
        state: Arc<Mutex<PluginState>>,
    ) -> Self {
        let world_id = world.get_id();
        let reserved_blocks = cuboid.volume().saturating_mul(2);
        Self {
            owner,
            world,
            world_id,
            kind: EditKind::Move(MoveState::new(cuboid, offset)),
            positions: EditPositions::empty(),
            history: Vec::new(),
            state,
            remaining: reserved_blocks,
            chunk_cursor: None,
            pattern_seed: 0,
        }
    }

    /// Creates a deferred history replay, traversing changes backward for undo and forward for redo.
    pub fn replay(
        owner: String,
        world: World,
        history: HistoryEntry,
        direction: ReplayDirection,
        state: Arc<Mutex<PluginState>>,
    ) -> Self {
        let remaining = history.len() as u64;
        let next_index = match direction {
            ReplayDirection::Undo => history.len(),
            ReplayDirection::Redo => 0,
        };
        Self {
            owner,
            world,
            world_id: history.world_id().to_owned(),
            kind: EditKind::Replay {
                history,
                direction,
                next_index,
            },
            positions: EditPositions::empty(),
            history: Vec::new(),
            state,
            remaining,
            chunk_cursor: None,
            pattern_seed: 0,
        }
    }

    fn pending_blocks(&self) -> u64 {
        match &self.kind {
            EditKind::Set { .. } | EditKind::Replace { .. } => self.remaining,
            EditKind::Move(_) => self.remaining,
            EditKind::Replay {
                history,
                direction,
                next_index,
            } => match direction {
                ReplayDirection::Undo => *next_index as u64,
                ReplayDirection::Redo => history.len().saturating_sub(*next_index) as u64,
            },
        }
    }

    fn process(&mut self, budget: usize, config: &Config) -> ProcessResult {
        let writer = WriteStrategy::new(config);
        if let EditKind::Replay {
            history,
            direction,
            next_index,
        } = &mut self.kind
        {
            return process_replay(
                &self.world,
                history,
                *direction,
                next_index,
                budget,
                &writer,
                &mut self.chunk_cursor,
            );
        }

        if let EditKind::Move(move_state) = &mut self.kind {
            let mut access = WorldAccess::new(&self.world, &mut self.chunk_cursor, &writer);
            return move_state.process(
                &mut access,
                &mut self.history,
                budget,
                config.max_history_blocks,
            );
        }

        match self.kind {
            EditKind::Set { ref to } => {
                self.process_forward(budget, config, &writer, None, to.clone())
            }
            EditKind::Replace { from, ref to } => {
                self.process_forward(budget, config, &writer, Some(from), to.clone())
            }
            EditKind::Move(_) => unreachable!("move handled above"),
            EditKind::Replay { .. } => unreachable!("history replay handled above"),
        }
    }

    fn finish(self) {
        match self.kind {
            EditKind::Set { .. } | EditKind::Replace { .. } | EditKind::Move(_) => {
                let history = HistoryEntry::new(self.world_id, self.history);
                if history.len() > 0 {
                    self.state
                        .lock()
                        .unwrap()
                        .push_undo_history(self.owner, history);
                }
            }
            EditKind::Replay {
                history, direction, ..
            } => {
                let mut state = self.state.lock().unwrap();
                match direction {
                    ReplayDirection::Undo => state.push_redo_history(self.owner, history),
                    ReplayDirection::Redo => state.push_replayed_undo_history(self.owner, history),
                }
            }
        }
    }

    /// Visits at most `budget` positions, applying the pattern and recording changes up to the history limit.
    fn process_forward(
        &mut self,
        budget: usize,
        config: &Config,
        writer: &WriteStrategy,
        replace_from: Option<u16>,
        pattern: Arc<BlockPattern>,
    ) -> ProcessResult {
        let mut visited = 0;

        while visited < budget {
            let Some(pos) = self.positions.next() else {
                return ProcessResult::Finished { scanned: visited };
            };

            visited += 1;
            let to = pattern.choose(pos, self.pattern_seed);
            let mut access = WorldAccess::new(&self.world, &mut self.chunk_cursor, writer);
            if let Some(change) = apply_forward(
                &mut access,
                pos,
                to,
                replace_from,
                self.history.len() < config.max_history_blocks,
            ) {
                self.history.push(change);
            }
            self.remaining = self.remaining.saturating_sub(1);
        }

        ProcessResult::Pending { scanned: visited }
    }
}

enum EditPositions {
    Cuboid(CuboidIter),
    Walls(WallIter),
    Vec(std::vec::IntoIter<BlockPos>),
}

impl EditPositions {
    fn cuboid(cuboid: Cuboid) -> Self {
        Self::Cuboid(cuboid.iter())
    }

    fn empty() -> Self {
        Self::Vec(Vec::new().into_iter())
    }
}

impl Iterator for EditPositions {
    type Item = BlockPos;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Cuboid(iter) => iter.next(),
            Self::Walls(iter) => iter.next(),
            Self::Vec(iter) => iter.next(),
        }
    }
}

#[derive(Default)]
pub struct EditQueue {
    queue: VecDeque<EditOperation>,
    queued_blocks: u64,
}

impl EditQueue {
    pub fn can_enqueue(&self, blocks: u64, config: &Config) -> Result<(), String> {
        if self.queue.len() >= config.max_queued_operations {
            return Err(format!(
                "Too many edits are already queued ({}/{}).",
                self.queue.len(),
                config.max_queued_operations
            ));
        }

        let Some(total_blocks) = self.queued_blocks.checked_add(blocks) else {
            return Err("Too many blocks are queued.".to_owned());
        };
        if total_blocks > config.max_queued_blocks {
            return Err(format!(
                "Too many blocks are already queued: {} queued, {blocks} new, limit is {}.",
                self.queued_blocks, config.max_queued_blocks
            ));
        }

        Ok(())
    }

    pub fn enqueue(&mut self, operation: EditOperation) {
        self.queued_blocks = self
            .queued_blocks
            .saturating_add(operation.pending_blocks());
        self.queue.push_back(operation);
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn queued_blocks(&self) -> u64 {
        self.queued_blocks
    }

    pub fn process_tick(&mut self, _server: &Server, config: &Config) {
        let budget = config.blocks_per_tick;
        if budget == 0 {
            return;
        }

        let Some(operation) = self.queue.front_mut() else {
            return;
        };

        let result = operation.process(budget, config);
        let released = released_reservation(
            matches!(operation.kind, EditKind::Move(_)),
            operation.pending_blocks(),
            &result,
        );
        self.queued_blocks = self.queued_blocks.saturating_sub(released);
        if result.is_finished() {
            if let Some(operation) = self.queue.pop_front() {
                operation.finish();
            }
        }
    }
}

enum ProcessResult {
    Pending { scanned: usize },
    Finished { scanned: usize },
}

fn released_reservation(is_move: bool, reserved: u64, result: &ProcessResult) -> u64 {
    if is_move {
        if result.is_finished() {
            reserved
        } else {
            0
        }
    } else {
        result.scanned()
    }
}

impl ProcessResult {
    fn scanned(&self) -> u64 {
        match self {
            Self::Pending { scanned } | Self::Finished { scanned } => *scanned as u64,
        }
    }

    fn is_finished(&self) -> bool {
        matches!(self, Self::Finished { .. })
    }
}

/// Replays at most `budget` history changes, restoring both block states and entity snapshots.
fn process_replay(
    world: &World,
    history: &mut HistoryEntry,
    direction: ReplayDirection,
    next_index: &mut usize,
    budget: usize,
    writer: &WriteStrategy,
    chunk_cursor: &mut Option<ChunkCursor>,
) -> ProcessResult {
    let mut visited = 0;
    while visited < budget {
        let Some(change) = next_replay_change(history, direction, next_index) else {
            return ProcessResult::Finished { scanned: visited };
        };
        replay_change(
            &mut WorldAccess::new(world, chunk_cursor, writer),
            change,
            direction,
        );
        visited += 1;
    }
    ProcessResult::Pending { scanned: visited }
}

fn next_replay_change<'a>(
    history: &'a HistoryEntry,
    direction: ReplayDirection,
    next_index: &mut usize,
) -> Option<&'a BlockChange> {
    match direction {
        ReplayDirection::Undo => {
            *next_index = next_index.checked_sub(1)?;
            history.changes.get(*next_index)
        }
        ReplayDirection::Redo => {
            let change = history.changes.get(*next_index)?;
            *next_index += 1;
            Some(change)
        }
    }
}

fn next_pattern_seed() -> u64 {
    PATTERN_SEED.fetch_add(1, Ordering::Relaxed)
}

/// Hashes signed block coordinates with an edit seed for deterministic pattern selection.
pub(super) fn position_hash(pos: BlockPos, seed: u64) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64 ^ seed;
    for value in [pos.x, pos.y, pos.z] {
        for byte in value.to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::{released_reservation, BlockPos, Cuboid, ProcessResult};
    use std::collections::HashSet;

    #[test]
    fn cuboid_normalizes_and_counts_volume() {
        let cuboid = Cuboid::new(BlockPos { x: 2, y: 4, z: 6 }, BlockPos { x: 1, y: 3, z: 5 });

        assert_eq!(cuboid.volume(), 8);
    }

    #[test]
    fn move_reservation_is_held_until_the_entire_move_finishes() {
        assert_eq!(
            released_reservation(true, 100, &ProcessResult::Pending { scanned: 40 }),
            0
        );
        assert_eq!(
            released_reservation(true, 100, &ProcessResult::Finished { scanned: 7 }),
            100
        );
        assert_eq!(
            released_reservation(false, 100, &ProcessResult::Pending { scanned: 40 }),
            40
        );
    }

    #[test]
    fn cuboid_iterates_x_then_z_then_y() {
        let cuboid = Cuboid::new(BlockPos { x: 0, y: 0, z: 0 }, BlockPos { x: 1, y: 0, z: 1 });
        let positions: Vec<_> = cuboid.iter().collect();

        assert_eq!(
            positions,
            vec![
                BlockPos { x: 0, y: 0, z: 0 },
                BlockPos { x: 1, y: 0, z: 0 },
                BlockPos { x: 0, y: 0, z: 1 },
                BlockPos { x: 1, y: 0, z: 1 },
            ]
        );
    }

    #[test]
    fn wall_positions_only_include_vertical_faces() {
        let cuboid = Cuboid::new(pos(0, 0, 0), pos(2, 2, 2));
        let positions = cuboid.wall_positions();

        assert_eq!(positions.len(), 24);
        assert!(positions.contains(&pos(0, 1, 1)));
        assert!(positions.contains(&pos(2, 1, 1)));
        assert!(positions.contains(&pos(1, 1, 0)));
        assert!(positions.contains(&pos(1, 1, 2)));
        assert!(!positions.contains(&pos(1, 0, 1)));
        assert!(!positions.contains(&pos(1, 2, 1)));
        assert_unique(&positions);
    }

    #[test]
    fn wall_positions_do_not_duplicate_thin_selections() {
        let cuboid = Cuboid::new(pos(0, 0, 0), pos(0, 2, 2));
        let positions = cuboid.wall_positions();

        assert_eq!(positions.len(), 9);
        assert_unique(&positions);
    }

    #[test]
    fn wall_positions_include_one_block_tall_perimeter() {
        let cuboid = Cuboid::new(pos(0, 5, 0), pos(2, 5, 2));
        let positions = cuboid.wall_positions();

        assert_eq!(positions.len(), 8);
        assert!(positions.contains(&pos(0, 5, 0)));
        assert!(positions.contains(&pos(1, 5, 0)));
        assert!(positions.contains(&pos(2, 5, 2)));
        assert!(!positions.contains(&pos(1, 5, 1)));
        assert_unique(&positions);
    }

    /// Checks that wall counts match generated positions for thin and ordinary cuboids.
    #[test]
    fn wall_counts_match_positions_for_thin_and_normal_selections() {
        for x in 1..=4 {
            for y in 1..=4 {
                for z in 1..=4 {
                    let cuboid = Cuboid::new(pos(0, 0, 0), pos(x - 1, y - 1, z - 1));
                    assert_eq!(cuboid.wall_volume(), cuboid.wall_positions().len() as u64);
                }
            }
        }
    }

    /// Checks that extreme cuboid counts saturate instead of overflowing or allocating walls.
    #[test]
    fn huge_selections_saturate_without_overflow_or_allocating_walls() {
        let cuboid = Cuboid::new(
            pos(i32::MIN, i32::MIN, i32::MIN),
            pos(i32::MAX, i32::MAX, i32::MAX),
        );
        assert_eq!(cuboid.volume(), u64::MAX);
        assert_eq!(cuboid.wall_volume(), u64::MAX);
    }

    fn pos(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

    fn assert_unique(positions: &[BlockPos]) {
        let unique: HashSet<_> = positions.iter().copied().collect();
        assert_eq!(unique.len(), positions.len());
    }
}
