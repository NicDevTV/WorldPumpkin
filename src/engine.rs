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
    sync::{Arc, Mutex},
};

mod generated_blocks {
    include!(concat!(env!("OUT_DIR"), "/block_states.rs"));
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

    pub fn iter(self) -> CuboidIter {
        CuboidIter {
            cuboid: self,
            next: Some(self.min),
        }
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
}

pub enum EditKind {
    Set { to: u16 },
    Replace { from: u16, to: u16 },
    Undo { history: HistoryEntry },
}

pub struct EditOperation {
    owner: String,
    world: World,
    world_id: String,
    kind: EditKind,
    positions: CuboidIter,
    history: Vec<BlockChange>,
    state: Arc<Mutex<PluginState>>,
    remaining: u64,
    chunk_cursor: Option<ChunkCursor>,
}

impl EditOperation {
    pub fn set(
        owner: String,
        world: World,
        cuboid: Cuboid,
        to: u16,
        state: Arc<Mutex<PluginState>>,
    ) -> Self {
        let world_id = world.get_id();
        Self {
            owner,
            world,
            world_id,
            kind: EditKind::Set { to },
            positions: cuboid.iter(),
            history: Vec::new(),
            state,
            remaining: cuboid.volume(),
            chunk_cursor: None,
        }
    }

    pub fn replace(
        owner: String,
        world: World,
        cuboid: Cuboid,
        from: u16,
        to: u16,
        state: Arc<Mutex<PluginState>>,
    ) -> Self {
        let world_id = world.get_id();
        Self {
            owner,
            world,
            world_id,
            kind: EditKind::Replace { from, to },
            positions: cuboid.iter(),
            history: Vec::new(),
            state,
            remaining: cuboid.volume(),
            chunk_cursor: None,
        }
    }

    pub fn undo(owner: String, world: World, history: HistoryEntry) -> Self {
        let world_id = world.get_id();
        let remaining = history.len() as u64;
        let cuboid = Cuboid::new(BlockPos { x: 0, y: 0, z: 0 }, BlockPos { x: 0, y: 0, z: 0 });
        Self {
            owner,
            world,
            world_id,
            kind: EditKind::Undo { history },
            positions: cuboid.iter(),
            history: Vec::new(),
            state: Arc::new(Mutex::new(PluginState::new(Config::default()))),
            remaining,
            chunk_cursor: None,
        }
    }

    fn pending_blocks(&self) -> u64 {
        match &self.kind {
            EditKind::Set { .. } | EditKind::Replace { .. } => self.remaining,
            EditKind::Undo { history } => history.len() as u64,
        }
    }

    fn process(&mut self, budget: usize, config: &Config) -> ProcessResult {
        let writer = WriteStrategy::new(config);
        if let EditKind::Undo { history } = &mut self.kind {
            return process_undo(&self.world, history, budget, &writer);
        }

        match self.kind {
            EditKind::Set { to } => self.process_forward(budget, config, &writer, None, to),
            EditKind::Replace { from, to } => {
                self.process_forward(budget, config, &writer, Some(from), to)
            }
            EditKind::Undo { .. } => unreachable!("undo handled above"),
        }
    }

    fn process_forward(
        &mut self,
        budget: usize,
        config: &Config,
        writer: &WriteStrategy,
        replace_from: Option<u16>,
        to: u16,
    ) -> ProcessResult {
        let mut visited = 0;

        while visited < budget {
            let Some(pos) = self.positions.next() else {
                self.state.lock().unwrap().push_history(
                    self.owner.clone(),
                    HistoryEntry::new(self.world_id.clone(), std::mem::take(&mut self.history)),
                );
                return ProcessResult::Finished { scanned: visited };
            };

            visited += 1;
            let old_state = writer.get_block_state_id(&self.world, &mut self.chunk_cursor, pos);
            if replace_from.is_none_or(|from| from == old_state) && old_state != to {
                writer.set_block_state(&self.world, &mut self.chunk_cursor, pos, to);
                if self.history.len() < config.max_history_blocks {
                    self.history.push(BlockChange { pos, old_state });
                }
            }
            self.remaining = self.remaining.saturating_sub(1);
        }

        ProcessResult::Pending { scanned: visited }
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
                "Edit queue is full: {}/{} operations are queued.",
                self.queue.len(),
                config.max_queued_operations
            ));
        }

        let Some(total_blocks) = self.queued_blocks.checked_add(blocks) else {
            return Err("Edit queue block count overflowed.".to_owned());
        };
        if total_blocks > config.max_queued_blocks {
            return Err(format!(
                "Edit queue is full: {} queued blocks, {blocks} new blocks, limit is {}.",
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
            self.queue.pop_front();
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

fn process_undo(
    world: &World,
    history: &mut HistoryEntry,
    budget: usize,
    writer: &WriteStrategy,
) -> ProcessResult {
    let mut visited = 0;
    let mut chunk_cursor = None;
    while visited < budget {
        let Some(change) = history.changes.pop() else {
            return ProcessResult::Finished { scanned: visited };
        };
        writer.set_block_state(world, &mut chunk_cursor, change.pos, change.old_state);
        visited += 1;
    }
    ProcessResult::Pending { scanned: visited }
}

impl WriteStrategy {
    fn new(config: &Config) -> Self {
        Self {
            // Direct chunk writes avoid world-level neighbor update paths.
            direct_chunk_writes: config.fast_mode && config.notify_clients,
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

fn block_state_key(input: &str) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    input.as_bytes().iter().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}

#[cfg(test)]
mod tests {
    use super::{local_chunk_pos, parse_block_state, BlockPos, Cuboid};

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
}
