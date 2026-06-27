// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use crate::{config::Config, state::PluginState};
use pumpkin_plugin_api::{
    common::BlockPos as WitBlockPos,
    server::Server,
    world::{BlockFlags, Chunk, World},
};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

mod generated_blocks {
    include!(concat!(env!("OUT_DIR"), "/block_states.rs"));
}

// Patterns are deterministic inside one edit, but each queued edit gets a fresh distribution.
static PATTERN_SEED: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl From<WitBlockPos> for BlockPos {
    fn from(pos: WitBlockPos) -> Self {
        Self {
            x: pos.x,
            y: pos.y,
            z: pos.z,
        }
    }
}

impl From<BlockPos> for WitBlockPos {
    fn from(pos: BlockPos) -> Self {
        Self {
            x: pos.x,
            y: pos.y,
            z: pos.z,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Selection {
    pub pos1: Option<BlockPos>,
    pub pos2: Option<BlockPos>,
}

impl Selection {
    pub fn cuboid(self) -> Option<Cuboid> {
        Some(Cuboid::new(self.pos1?, self.pos2?))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cuboid {
    min: BlockPos,
    max: BlockPos,
}

impl Cuboid {
    pub fn new(a: BlockPos, b: BlockPos) -> Self {
        Self {
            min: BlockPos {
                x: a.x.min(b.x),
                y: a.y.min(b.y),
                z: a.z.min(b.z),
            },
            max: BlockPos {
                x: a.x.max(b.x),
                y: a.y.max(b.y),
                z: a.z.max(b.z),
            },
        }
    }

    pub fn volume(self) -> u64 {
        let x = (self.max.x - self.min.x + 1) as u64;
        let y = (self.max.y - self.min.y + 1) as u64;
        let z = (self.max.z - self.min.z + 1) as u64;
        x * y * z
    }

    pub fn wall_volume(self) -> u64 {
        self.wall_positions().len() as u64
    }

    pub fn iter(self) -> CuboidIter {
        CuboidIter {
            cuboid: self,
            next: Some(self.min),
        }
    }

    pub fn wall_positions(self) -> Vec<BlockPos> {
        self.iter()
            .filter(|pos| {
                pos.x == self.min.x
                    || pos.x == self.max.x
                    || pos.z == self.min.z
                    || pos.z == self.max.z
            })
            .collect()
    }
}

pub struct CuboidIter {
    cuboid: Cuboid,
    next: Option<BlockPos>,
}

impl Iterator for CuboidIter {
    type Item = BlockPos;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next?;
        self.next = advance_position(self.cuboid, current);
        Some(current)
    }
}

fn advance_position(cuboid: Cuboid, current: BlockPos) -> Option<BlockPos> {
    if current.x < cuboid.max.x {
        return Some(BlockPos {
            x: current.x + 1,
            ..current
        });
    }
    if current.z < cuboid.max.z {
        return Some(BlockPos {
            x: cuboid.min.x,
            z: current.z + 1,
            ..current
        });
    }
    if current.y < cuboid.max.y {
        return Some(BlockPos {
            x: cuboid.min.x,
            y: current.y + 1,
            z: cuboid.min.z,
        });
    }
    None
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Copy, Debug)]
pub struct BlockChange {
    pos: BlockPos,
    old_state: u16,
    new_state: u16,
}

#[derive(Clone, Debug)]
pub struct BlockPattern {
    choices: Vec<WeightedBlock>,
    total_weight: u32,
}

#[derive(Clone, Copy, Debug)]
struct WeightedBlock {
    state: u16,
    weight: u32,
}

impl BlockPattern {
    fn choose(&self, pos: BlockPos, seed: u64) -> u16 {
        let mut cursor = position_hash(pos, seed) % u64::from(self.total_weight);
        for choice in &self.choices {
            let weight = u64::from(choice.weight);
            if cursor < weight {
                return choice.state;
            }
            cursor -= weight;
        }
        self.choices
            .last()
            .map(|choice| choice.state)
            .unwrap_or_default()
    }
}

pub enum EditKind {
    Set {
        to: BlockPattern,
    },
    Replace {
        from: u16,
        to: BlockPattern,
    },
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
            kind: EditKind::Set { to },
            positions: EditPositions::cuboid(cuboid),
            history: Vec::new(),
            state,
            remaining: cuboid.volume(),
            chunk_cursor: None,
            pattern_seed: next_pattern_seed(),
        }
    }

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
            kind: EditKind::Replace { from, to },
            positions: EditPositions::cuboid(cuboid),
            history: Vec::new(),
            state,
            remaining: cuboid.volume(),
            chunk_cursor: None,
            pattern_seed: next_pattern_seed(),
        }
    }

    pub fn walls(
        owner: String,
        world: World,
        cuboid: Cuboid,
        to: BlockPattern,
        state: Arc<Mutex<PluginState>>,
    ) -> Self {
        let world_id = world.get_id();
        let positions = cuboid.wall_positions();
        let remaining = positions.len() as u64;
        Self {
            owner,
            world,
            world_id,
            kind: EditKind::Set { to },
            positions: EditPositions::vec(positions),
            history: Vec::new(),
            state,
            remaining,
            chunk_cursor: None,
            pattern_seed: next_pattern_seed(),
        }
    }

    pub fn undo(
        owner: String,
        world: World,
        history: HistoryEntry,
        state: Arc<Mutex<PluginState>>,
    ) -> Self {
        let world_id = world.get_id();
        let remaining = history.len() as u64;
        Self {
            owner,
            world,
            world_id,
            kind: EditKind::Replay {
                next_index: history.len(),
                history,
                direction: ReplayDirection::Undo,
            },
            positions: EditPositions::empty(),
            history: Vec::new(),
            state,
            remaining,
            chunk_cursor: None,
            pattern_seed: 0,
        }
    }

    pub fn redo(
        owner: String,
        world: World,
        history: HistoryEntry,
        state: Arc<Mutex<PluginState>>,
    ) -> Self {
        let world_id = world.get_id();
        let remaining = history.len() as u64;
        Self {
            owner,
            world,
            world_id,
            kind: EditKind::Replay {
                history,
                direction: ReplayDirection::Redo,
                next_index: 0,
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

        match self.kind {
            EditKind::Set { ref to } => {
                self.process_forward(budget, config, &writer, None, to.clone())
            }
            EditKind::Replace { from, ref to } => {
                self.process_forward(budget, config, &writer, Some(from), to.clone())
            }
            EditKind::Replay { .. } => unreachable!("history replay handled above"),
        }
    }

    fn finish(&mut self) {
        match &self.kind {
            EditKind::Set { .. } | EditKind::Replace { .. } => {
                let history =
                    HistoryEntry::new(self.world_id.clone(), std::mem::take(&mut self.history));
                if history.len() > 0 {
                    self.state
                        .lock()
                        .unwrap()
                        .push_undo_history(self.owner.clone(), history);
                }
            }
            EditKind::Replay {
                history, direction, ..
            } => {
                let history = history.clone();
                let mut state = self.state.lock().unwrap();
                match direction {
                    ReplayDirection::Undo => state.push_redo_history(self.owner.clone(), history),
                    ReplayDirection::Redo => {
                        state.push_replayed_undo_history(self.owner.clone(), history)
                    }
                }
            }
        }
    }

    fn process_forward(
        &mut self,
        budget: usize,
        config: &Config,
        writer: &WriteStrategy,
        replace_from: Option<u16>,
        pattern: BlockPattern,
    ) -> ProcessResult {
        let mut visited = 0;

        while visited < budget {
            let Some(pos) = self.positions.next() else {
                return ProcessResult::Finished { scanned: visited };
            };

            visited += 1;
            let to = pattern.choose(pos, self.pattern_seed);
            let old_state = writer.get_block_state_id(&self.world, &mut self.chunk_cursor, pos);
            if replace_from.is_none_or(|from| from == old_state) && old_state != to {
                writer.set_block_state(&self.world, &mut self.chunk_cursor, pos, to);
                if self.history.len() < config.max_history_blocks {
                    self.history.push(BlockChange {
                        pos,
                        old_state,
                        new_state: to,
                    });
                }
            }
            self.remaining = self.remaining.saturating_sub(1);
        }

        ProcessResult::Pending { scanned: visited }
    }
}

enum EditPositions {
    Cuboid(CuboidIter),
    Vec(std::vec::IntoIter<BlockPos>),
}

impl EditPositions {
    fn cuboid(cuboid: Cuboid) -> Self {
        Self::Cuboid(cuboid.iter())
    }

    fn vec(positions: Vec<BlockPos>) -> Self {
        Self::Vec(positions.into_iter())
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
        self.queued_blocks = self.queued_blocks.saturating_sub(result.scanned());
        if result.is_finished() {
            if let Some(mut operation) = self.queue.pop_front() {
                operation.finish();
            }
        }
    }
}

enum ProcessResult {
    Pending { scanned: usize },
    Finished { scanned: usize },
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

struct ChunkCursor {
    x: i32,
    z: i32,
    chunk: Chunk,
}

struct WriteStrategy {
    direct_chunk_writes: bool,
    fallback_flags: BlockFlags,
}

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
        let state = match direction {
            ReplayDirection::Undo => change.old_state,
            ReplayDirection::Redo => change.new_state,
        };
        writer.set_block_state(world, chunk_cursor, change.pos, state);
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

impl WriteStrategy {
    fn new(config: &Config) -> Self {
        Self {
            // Direct chunk writes avoid world-level neighbor update paths.
            direct_chunk_writes: config.fast_mode,
            fallback_flags: block_flags(config),
        }
    }

    fn get_block_state_id(
        &self,
        world: &World,
        chunk_cursor: &mut Option<ChunkCursor>,
        pos: BlockPos,
    ) -> u16 {
        self.chunk(world, chunk_cursor, pos).map_or_else(
            || world.get_block_state_id(pos.into()),
            |chunk| chunk.get_block_state_id(local_chunk_pos(pos)),
        )
    }

    fn set_block_state(
        &self,
        world: &World,
        chunk_cursor: &mut Option<ChunkCursor>,
        pos: BlockPos,
        state: u16,
    ) {
        if let Some(chunk) = self.chunk(world, chunk_cursor, pos) {
            chunk.set_block_state(local_chunk_pos(pos), state);
            return;
        }

        world.set_block_state(pos.into(), state, self.fallback_flags);
    }

    fn chunk<'a>(
        &self,
        world: &World,
        chunk_cursor: &'a mut Option<ChunkCursor>,
        pos: BlockPos,
    ) -> Option<&'a Chunk> {
        if !self.direct_chunk_writes {
            return None;
        }

        let chunk_x = pos.x.div_euclid(16);
        let chunk_z = pos.z.div_euclid(16);
        let cached = chunk_cursor
            .as_ref()
            .is_some_and(|cursor| cursor.x == chunk_x && cursor.z == chunk_z);
        if !cached {
            *chunk_cursor = world.get_chunk(chunk_x, chunk_z).map(|chunk| ChunkCursor {
                x: chunk_x,
                z: chunk_z,
                chunk,
            });
        }

        chunk_cursor.as_ref().map(|cursor| &cursor.chunk)
    }
}

fn local_chunk_pos(pos: BlockPos) -> WitBlockPos {
    WitBlockPos {
        x: pos.x.rem_euclid(16),
        y: pos.y,
        z: pos.z.rem_euclid(16),
    }
}

fn block_flags(config: &Config) -> BlockFlags {
    let mut flags = BlockFlags::empty();

    if config.notify_clients {
        flags = flags | BlockFlags::NOTIFY_LISTENERS;
    }

    if !config.fast_mode {
        flags = flags | BlockFlags::NOTIFY_NEIGHBORS;
    } else {
        // Fallback path for unloaded chunks: keep it no-physics as far as Pumpkin allows.
        flags = flags
            | BlockFlags::SKIP_DROPS
            | BlockFlags::SKIP_REDSTONE_WIRE_STATE_REPLACEMENT
            | BlockFlags::SKIP_BLOCK_ENTITY_REPLACED_CALLBACK
            | BlockFlags::SKIP_BLOCK_ADDED_CALLBACK;
    }

    flags
}

pub fn parse_block_state(input: &str) -> Result<u16, String> {
    let trimmed = input.trim();
    if let Ok(state_id) = trimmed.parse::<u16>() {
        return Ok(state_id);
    }
    let trimmed = trimmed.strip_prefix("minecraft:").unwrap_or(trimmed);
    let key = block_state_key(trimmed);

    generated_blocks::BLOCK_STATES
        .binary_search_by_key(&key, |(key, _)| *key)
        .map(|index| generated_blocks::BLOCK_STATES[index].1)
        .map_err(|_| format!("unknown block state `{input}`"))
}

pub fn parse_block_pattern(input: &str) -> Result<BlockPattern, String> {
    let mut choices = Vec::new();
    let mut total_weight = 0_u32;

    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err("empty block in pattern".to_owned());
        }
        let (weight, block) = parse_weighted_block(part)?;
        let state = parse_block_state(block)?;
        total_weight = total_weight
            .checked_add(weight)
            .ok_or_else(|| "pattern weights are too large".to_owned())?;
        choices.push(WeightedBlock { state, weight });
    }

    if choices.is_empty() {
        return Err("empty block pattern".to_owned());
    }

    Ok(BlockPattern {
        choices,
        total_weight,
    })
}

fn parse_weighted_block(input: &str) -> Result<(u32, &str), String> {
    let Some((weight, block)) = input.split_once('%') else {
        return Ok((1, input));
    };
    let weight = weight
        .trim()
        .parse::<u32>()
        .map_err(|err| format!("invalid pattern weight `{}`: {err}", weight.trim()))?;
    if weight == 0 {
        return Err("pattern weights must be greater than 0".to_owned());
    }
    let block = block.trim();
    if block.is_empty() {
        return Err("missing block after pattern weight".to_owned());
    }
    Ok((weight, block))
}

fn next_pattern_seed() -> u64 {
    PATTERN_SEED.fetch_add(1, Ordering::Relaxed)
}

fn position_hash(pos: BlockPos, seed: u64) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64 ^ seed;
    for value in [pos.x, pos.y, pos.z] {
        for byte in value.to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn block_state_key(input: &str) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    input.as_bytes().iter().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}

#[cfg(test)]
mod tests {
    use super::{local_chunk_pos, parse_block_pattern, parse_block_state, BlockPos, Cuboid};
    use std::collections::HashSet;

    #[test]
    fn cuboid_normalizes_and_counts_volume() {
        let cuboid = Cuboid::new(BlockPos { x: 2, y: 4, z: 6 }, BlockPos { x: 1, y: 3, z: 5 });

        assert_eq!(cuboid.volume(), 8);
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

    #[test]
    fn block_parser_resolves_namespaced_default_state() {
        assert_eq!(parse_block_state("minecraft:stone").unwrap(), 1);
    }

    #[test]
    fn block_parser_resolves_generated_property_state() {
        assert!(parse_block_state("oak_log[axis=x]").is_ok());
    }

    #[test]
    fn block_parser_resolves_namespaced_property_state() {
        assert_eq!(
            parse_block_state("minecraft:oak_log[axis=x]").unwrap(),
            parse_block_state("oak_log[axis=x]").unwrap()
        );
    }

    #[test]
    fn block_parser_keeps_numeric_state_ids_available() {
        assert_eq!(parse_block_state("42").unwrap(), 42);
    }

    #[test]
    fn block_pattern_accepts_weighted_entries() {
        let pattern = parse_block_pattern("50%dirt,50%grass_block").unwrap();

        assert_eq!(pattern.total_weight, 100);
        assert_eq!(pattern.choices.len(), 2);
    }

    #[test]
    fn block_pattern_keeps_single_blocks_available() {
        let pattern = parse_block_pattern("stone").unwrap();

        assert_eq!(pattern.total_weight, 1);
        assert_eq!(pattern.choose(BlockPos { x: 0, y: 0, z: 0 }, 0), 1);
    }

    #[test]
    fn block_pattern_rejects_zero_weight() {
        assert!(parse_block_pattern("0%dirt,100%grass_block").is_err());
    }

    #[test]
    fn local_chunk_pos_handles_negative_coordinates() {
        let pos = local_chunk_pos(BlockPos {
            x: -1,
            y: 64,
            z: -17,
        });

        assert_eq!(pos.x, 15);
        assert_eq!(pos.y, 64);
        assert_eq!(pos.z, 15);
    }

    fn pos(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

    fn assert_unique(positions: &[BlockPos]) {
        let unique: HashSet<_> = positions.iter().copied().collect();
        assert_eq!(unique.len(), positions.len());
    }
}
