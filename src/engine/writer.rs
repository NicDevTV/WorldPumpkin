// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{BlockChange, BlockPos, ReplayDirection};
use crate::config::Config;
use pumpkin_plugin_api::{
    common::BlockPos as WitBlockPos,
    world::{self, BlockFlags, Chunk, World},
};
use std::{cell::RefCell, collections::HashMap};

pub(super) struct ChunkCursor {
    x: i32,
    z: i32,
    chunk: Chunk,
}

pub(super) struct WriteStrategy {
    entity_states: RefCell<HashMap<u16, bool>>,
    direct_chunk_writes: bool,
    fallback_flags: BlockFlags,
    entity_callback_flags: BlockFlags,
}

/// The edit/history logic also runs against an in-memory world in regression tests.
pub(super) trait BlockAccess {
    fn state(&mut self, pos: BlockPos) -> u16;
    fn has_entity(&self, state: u16) -> bool;
    fn entity(&self, pos: BlockPos) -> Option<Vec<u8>>;
    fn write_state(&mut self, pos: BlockPos, state: u16, entity_callbacks: bool);
    fn write_entity(&mut self, pos: BlockPos, nbt: Option<&[u8]>);
}

pub(super) struct WorldAccess<'a> {
    world: &'a World,
    chunk_cursor: &'a mut Option<ChunkCursor>,
    writer: &'a WriteStrategy,
}

impl<'a> WorldAccess<'a> {
    pub(super) fn new(
        world: &'a World,
        chunk_cursor: &'a mut Option<ChunkCursor>,
        writer: &'a WriteStrategy,
    ) -> Self {
        Self {
            world,
            chunk_cursor,
            writer,
        }
    }
}

impl BlockAccess for WorldAccess<'_> {
    fn state(&mut self, pos: BlockPos) -> u16 {
        self.writer
            .get_block_state_id(self.world, self.chunk_cursor, pos)
    }

    fn has_entity(&self, state: u16) -> bool {
        if let Some(has_entity) = self.writer.entity_states.borrow().get(&state) {
            return *has_entity;
        }
        let has_entity = world::get_block_state_by_id(state)
            .is_some_and(|state| state.block_entity_type != u16::MAX);
        self.writer
            .entity_states
            .borrow_mut()
            .insert(state, has_entity);
        has_entity
    }

    fn entity(&self, pos: BlockPos) -> Option<Vec<u8>> {
        self.world.get_block_entity_nbt(pos.into())
    }

    fn write_state(&mut self, pos: BlockPos, state: u16, entity_callbacks: bool) {
        if entity_callbacks {
            self.writer
                .set_block_state_with_entity_callbacks(self.world, pos, state);
        } else {
            self.writer
                .set_block_state(self.world, self.chunk_cursor, pos, state);
        }
    }

    fn write_entity(&mut self, pos: BlockPos, nbt: Option<&[u8]>) {
        self.writer.set_block_entity(self.world, pos, nbt);
    }
}

pub(super) fn apply_forward(
    access: &mut impl BlockAccess,
    pos: BlockPos,
    to: u16,
    replace_from: Option<u16>,
    record_history: bool,
) -> Option<BlockChange> {
    let old_state = access.state(pos);
    if replace_from.is_some_and(|from| from != old_state) || old_state == to {
        return None;
    }
    let old_has_entity = access.has_entity(old_state);
    let new_has_entity = access.has_entity(to);
    let old_block_entity = if record_history && old_has_entity {
        access.entity(pos)
    } else {
        None
    };
    access.write_state(pos, to, old_has_entity || new_has_entity);
    if !record_history {
        return None;
    }
    let new_block_entity = if new_has_entity {
        access.entity(pos)
    } else {
        None
    };
    Some(BlockChange {
        pos,
        old_state,
        new_state: to,
        old_block_entity,
        new_block_entity,
    })
}

pub(super) fn replay_change(
    access: &mut impl BlockAccess,
    change: &BlockChange,
    direction: ReplayDirection,
) {
    let (state, entity) = match direction {
        ReplayDirection::Undo => (change.old_state, change.old_block_entity.as_deref()),
        ReplayDirection::Redo => (change.new_state, change.new_block_entity.as_deref()),
    };
    let current_state = access.state(change.pos);
    let callbacks = access.has_entity(current_state)
        || access.has_entity(state)
        || change.old_block_entity.is_some()
        || change.new_block_entity.is_some();
    access.write_state(change.pos, state, callbacks);
    access.write_entity(change.pos, entity);
}

impl WriteStrategy {
    pub(super) fn new(config: &Config) -> Self {
        Self {
            entity_states: RefCell::new(HashMap::new()),
            // Direct chunk writes avoid world-level neighbor update paths.
            direct_chunk_writes: config.fast_mode,
            fallback_flags: block_flags(config),
            entity_callback_flags: block_entity_flags(config),
        }
    }

    pub(super) fn get_block_state_id(
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

    pub(super) fn set_block_state(
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

    pub(super) fn set_block_entity(&self, world: &World, pos: BlockPos, nbt: Option<&[u8]>) {
        let Some(nbt) = nbt else {
            return;
        };
        if let Err(error) = world.set_block_entity_nbt(pos.into(), nbt) {
            eprintln!("WorldPumpkin: failed to restore block entity at {pos:?}: {error}");
        }
    }

    pub(super) fn set_block_state_with_entity_callbacks(
        &self,
        world: &World,
        pos: BlockPos,
        state: u16,
    ) {
        world.set_block_state(pos.into(), state, self.entity_callback_flags);
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

fn block_entity_flags(config: &Config) -> BlockFlags {
    // Container replacement callbacks drop inventory items, which undo would duplicate.
    let mut flags = BlockFlags::FORCE_STATE | BlockFlags::SKIP_BLOCK_ENTITY_REPLACED_CALLBACK;

    if config.notify_clients {
        flags = flags | BlockFlags::NOTIFY_LISTENERS;
    }

    if !config.fast_mode {
        flags = flags | BlockFlags::NOTIFY_NEIGHBORS;
    } else {
        flags = flags | BlockFlags::SKIP_DROPS | BlockFlags::SKIP_REDSTONE_WIRE_STATE_REPLACEMENT;
    }

    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::BlockSnapshot;

    const POS: BlockPos = BlockPos { x: 1, y: 64, z: 2 };
    const STONE: u16 = 1;
    const CHEST: u16 = 2;
    const FURNACE: u16 = 3;

    struct WorldStub {
        block: BlockSnapshot,
        callback_writes: Vec<bool>,
        entity_reads: std::cell::Cell<usize>,
    }

    impl WorldStub {
        fn new(state: u16, block_entity: Option<Vec<u8>>) -> Self {
            Self {
                block: BlockSnapshot {
                    state,
                    block_entity,
                },
                callback_writes: vec![],
                entity_reads: std::cell::Cell::new(0),
            }
        }
    }

    impl BlockAccess for WorldStub {
        fn state(&mut self, _: BlockPos) -> u16 {
            self.block.state
        }
        fn has_entity(&self, state: u16) -> bool {
            matches!(state, CHEST | FURNACE)
        }
        fn entity(&self, _: BlockPos) -> Option<Vec<u8>> {
            self.entity_reads.set(self.entity_reads.get() + 1);
            self.block.block_entity.clone()
        }
        fn write_state(&mut self, _: BlockPos, state: u16, callbacks: bool) {
            self.callback_writes.push(callbacks);
            if callbacks && self.block.state != state {
                self.block.block_entity = if self.has_entity(state) {
                    Some(vec![0])
                } else {
                    None
                };
            }
            self.block.state = state;
        }
        fn write_entity(&mut self, _: BlockPos, nbt: Option<&[u8]>) {
            if let Some(nbt) = nbt {
                self.block.block_entity = Some(nbt.to_vec());
            }
        }
    }

    #[test]
    fn set_undo_restores_inventory_and_redo_removes_it() {
        let inventory = vec![1, 7, 42];
        let mut world = WorldStub::new(CHEST, Some(inventory.clone()));
        let change = apply_forward(&mut world, POS, STONE, None, true).unwrap();
        assert_eq!(world.block.block_entity, None);
        replay_change(&mut world, &change, ReplayDirection::Undo);
        assert_eq!(world.block.state, CHEST);
        assert_eq!(world.block.block_entity, Some(inventory));
        replay_change(&mut world, &change, ReplayDirection::Redo);
        assert_eq!(
            world.block,
            BlockSnapshot {
                state: STONE,
                block_entity: None
            }
        );
        assert!(world.callback_writes.iter().all(|callbacks| *callbacks));
    }

    #[test]
    fn replace_keeps_both_entity_snapshots() {
        let mut world = WorldStub::new(CHEST, Some(vec![1, 7]));
        let change = apply_forward(&mut world, POS, FURNACE, Some(CHEST), true).unwrap();
        assert_eq!(change.old_block_entity, Some(vec![1, 7]));
        assert_eq!(change.new_block_entity, Some(vec![0]));
        replay_change(&mut world, &change, ReplayDirection::Undo);
        assert_eq!(world.block.block_entity, Some(vec![1, 7]));
        replay_change(&mut world, &change, ReplayDirection::Redo);
        assert_eq!(
            world.block,
            BlockSnapshot {
                state: FURNACE,
                block_entity: Some(vec![0])
            }
        );
    }

    #[test]
    fn placing_an_entity_records_the_new_nbt_for_redo() {
        let mut world = WorldStub::new(STONE, None);
        let change = apply_forward(&mut world, POS, CHEST, None, true).unwrap();
        assert_eq!(change.new_block_entity, Some(vec![0]));
        replay_change(&mut world, &change, ReplayDirection::Undo);
        assert_eq!(world.block.block_entity, None);
        replay_change(&mut world, &change, ReplayDirection::Redo);
        assert_eq!(world.block.block_entity, Some(vec![0]));
    }

    #[test]
    fn unchanged_blocks_and_replace_misses_keep_inventory() {
        let mut world = WorldStub::new(CHEST, Some(vec![42]));
        assert!(apply_forward(&mut world, POS, CHEST, None, true).is_none());
        assert!(apply_forward(&mut world, POS, STONE, Some(FURNACE), true).is_none());
        assert_eq!(world.block.block_entity, Some(vec![42]));
        assert!(world.callback_writes.is_empty());
    }

    #[test]
    fn edits_still_run_when_history_is_full() {
        let mut world = WorldStub::new(CHEST, Some(vec![42]));
        assert!(apply_forward(&mut world, POS, STONE, None, false).is_none());
        assert_eq!(
            world.block,
            BlockSnapshot {
                state: STONE,
                block_entity: None
            }
        );
    }

    #[test]
    fn ordinary_blocks_keep_the_direct_write_path() {
        let mut world = WorldStub::new(STONE, None);
        apply_forward(&mut world, POS, 4, None, true).unwrap();
        assert_eq!(world.callback_writes, [false]);
        assert_eq!(world.entity_reads.get(), 0);
    }

    #[test]
    fn entity_writes_do_not_drop_inventory_in_either_mode() {
        for fast_mode in [true, false] {
            let flags = block_entity_flags(&Config {
                fast_mode,
                ..Config::default()
            });
            assert_ne!(
                (flags & BlockFlags::SKIP_BLOCK_ENTITY_REPLACED_CALLBACK).bits(),
                0
            );
            assert_eq!((flags & BlockFlags::SKIP_BLOCK_ADDED_CALLBACK).bits(), 0);
        }
    }

    #[test]
    fn chunk_positions_wrap_negative_coordinates() {
        let pos = local_chunk_pos(BlockPos {
            x: -1,
            y: 64,
            z: -17,
        });
        assert_eq!((pos.x, pos.y, pos.z), (15, 64, 15));
    }
}
