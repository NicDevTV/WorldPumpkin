// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use super::{
    geometry::CuboidIter, writer::BlockAccess, BlockChange, BlockPos, BlockSnapshot, Cuboid,
    ProcessResult,
};
use std::collections::HashMap;

/// Snapshot both regions before writing, then visit their union without allocating a move plan.
pub(super) struct MoveState {
    source: Cuboid,
    destination: Cuboid,
    offset: BlockPos,
    positions: CuboidIter,
    snapshots: HashMap<BlockPos, BlockSnapshot>,
    phase: Phase,
}

enum Phase {
    Snapshot,
    Source,
    Destination,
}

impl MoveState {
    pub(super) fn new(source: Cuboid, offset: BlockPos) -> Self {
        let destination = source
            .translated(offset)
            .expect("move destination was validated before enqueue");
        Self {
            source,
            destination,
            offset,
            positions: source.iter(),
            snapshots: HashMap::new(),
            phase: Phase::Snapshot,
        }
    }

    pub(super) fn process(
        &mut self,
        access: &mut impl BlockAccess,
        history: &mut Vec<BlockChange>,
        budget: usize,
        max_history: usize,
    ) -> ProcessResult {
        let mut scanned = 0;
        while scanned < budget {
            let Some(pos) = self.positions.next() else {
                match self.phase {
                    Phase::Snapshot => {
                        self.phase = Phase::Source;
                        self.positions = self.source.iter();
                    }
                    Phase::Source => {
                        self.phase = Phase::Destination;
                        self.positions = self.destination.iter();
                    }
                    Phase::Destination => return ProcessResult::Finished { scanned },
                }
                continue;
            };
            scanned += 1;
            match self.phase {
                Phase::Snapshot => {
                    self.snapshot(access, pos);
                    let target = pos
                        .checked_offset(self.offset)
                        .expect("validated move coordinates");
                    self.snapshot(access, target);
                }
                Phase::Source => self.apply(access, history, max_history, pos),
                Phase::Destination => {
                    if !self.source.contains(pos) {
                        self.apply(access, history, max_history, pos);
                    }
                }
            }
        }
        ProcessResult::Pending { scanned }
    }

    fn snapshot(&mut self, access: &mut impl BlockAccess, pos: BlockPos) {
        self.snapshots.entry(pos).or_insert_with(|| {
            let state = access.state(pos);
            let block_entity = if access.has_entity(state) {
                access.entity(pos)
            } else {
                None
            };
            BlockSnapshot {
                state,
                block_entity,
            }
        });
    }

    fn apply(
        &self,
        access: &mut impl BlockAccess,
        history: &mut Vec<BlockChange>,
        max_history: usize,
        pos: BlockPos,
    ) {
        let old = self
            .snapshots
            .get(&pos)
            .expect("all affected positions were snapshotted");
        let empty = BlockSnapshot {
            state: 0,
            block_entity: None,
        };
        let new = if self.destination.contains(pos) {
            let original = pos
                .checked_sub(self.offset)
                .expect("validated move coordinates");
            self.snapshots
                .get(&original)
                .expect("all source positions were snapshotted")
        } else {
            &empty
        };
        if old == new {
            return;
        }

        let callbacks = access.has_entity(old.state)
            || access.has_entity(new.state)
            || old.block_entity.is_some()
            || new.block_entity.is_some();
        access.write_state(pos, new.state, callbacks);
        access.write_entity(pos, new.block_entity.as_deref());
        if history.len() < max_history {
            history.push(BlockChange {
                pos,
                old_state: old.state,
                new_state: new.state,
                old_block_entity: old.block_entity.clone(),
                new_block_entity: new.block_entity.clone(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{writer::replay_change, ReplayDirection};
    use std::collections::HashMap;

    #[derive(Default)]
    struct TestWorld {
        blocks: HashMap<BlockPos, BlockSnapshot>,
        writes: usize,
        entity_reads: std::cell::Cell<usize>,
    }
    impl BlockAccess for TestWorld {
        fn state(&mut self, pos: BlockPos) -> u16 {
            self.blocks.get(&pos).map_or(0, |block| block.state)
        }
        fn has_entity(&self, state: u16) -> bool {
            state == 10
        }
        fn entity(&self, pos: BlockPos) -> Option<Vec<u8>> {
            self.entity_reads.set(self.entity_reads.get() + 1);
            self.blocks
                .get(&pos)
                .and_then(|block| block.block_entity.clone())
        }
        fn write_state(&mut self, pos: BlockPos, state: u16, _: bool) {
            self.writes += 1;
            self.blocks.insert(
                pos,
                BlockSnapshot {
                    state,
                    block_entity: None,
                },
            );
        }
        fn write_entity(&mut self, pos: BlockPos, nbt: Option<&[u8]>) {
            if let Some(nbt) = nbt {
                self.blocks.get_mut(&pos).unwrap().block_entity = Some(nbt.to_vec());
            }
        }
    }
    fn pos(x: i32) -> BlockPos {
        BlockPos { x, y: 0, z: 0 }
    }

    #[test]
    fn overlapping_moves_preserve_original_blocks_and_entity_data_with_tiny_budget() {
        for offset in [-1, 0, 1, 4] {
            let mut world = TestWorld::default();
            for (x, state) in [(0, 10), (1, 11), (2, 12)] {
                world.blocks.insert(
                    pos(x),
                    BlockSnapshot {
                        state,
                        block_entity: (state == 10).then_some(vec![4, 2]),
                    },
                );
            }
            let original = world.blocks.clone();
            let source = Cuboid::new(pos(0), pos(1));
            let mut operation = MoveState::new(source, pos(offset));
            let mut history = Vec::new();
            for tick in 0..20 {
                let result = operation.process(&mut world, &mut history, 1, 100);
                assert!(result.scanned() <= 1);
                if tick < 2 {
                    assert_eq!(world.writes, 0, "writes must wait for all snapshots");
                }
                if result.is_finished() {
                    break;
                }
                assert!(tick < 19, "move did not complete");
            }
            let mut expected = original.clone();
            for x in 0..=1 {
                expected.insert(
                    pos(x),
                    BlockSnapshot {
                        state: 0,
                        block_entity: None,
                    },
                );
            }
            for x in 0..=1 {
                expected.insert(pos(x + offset), original[&pos(x)].clone());
            }
            for x in -1..=5 {
                let air = BlockSnapshot {
                    state: 0,
                    block_entity: None,
                };
                assert_eq!(
                    world.blocks.get(&pos(x)).unwrap_or(&air),
                    expected.get(&pos(x)).unwrap_or(&air),
                    "offset {offset}, position {x}"
                );
            }
            assert_eq!(
                world.entity_reads.get(),
                1,
                "only the entity state needs an NBT read"
            );
            for change in history.iter().rev() {
                replay_change(&mut world, change, ReplayDirection::Undo);
            }
            for x in -1..=5 {
                let air = BlockSnapshot {
                    state: 0,
                    block_entity: None,
                };
                assert_eq!(
                    world.blocks.get(&pos(x)).unwrap_or(&air),
                    original.get(&pos(x)).unwrap_or(&air),
                    "undo offset {offset}, position {x}"
                );
            }
            for change in &history {
                replay_change(&mut world, change, ReplayDirection::Redo);
            }
            for x in -1..=5 {
                let air = BlockSnapshot {
                    state: 0,
                    block_entity: None,
                };
                assert_eq!(
                    world.blocks.get(&pos(x)).unwrap_or(&air),
                    expected.get(&pos(x)).unwrap_or(&air),
                    "redo offset {offset}, position {x}"
                );
            }
        }
    }

    #[test]
    fn three_dimensional_moves_match_snapshot_model_in_every_direction() {
        let source = Cuboid::new(
            BlockPos {
                x: -1,
                y: -1,
                z: -1,
            },
            BlockPos { x: 1, y: 1, z: 1 },
        );
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let offset = BlockPos {
                        x: dx,
                        y: dy,
                        z: dz,
                    };
                    let mut world = TestWorld::default();
                    for x in -2..=2 {
                        for y in -2..=2 {
                            for z in -2..=2 {
                                let state = if (x + y + z) % 4 == 0 {
                                    10
                                } else {
                                    11 + (x + 2) as u16
                                };
                                world.blocks.insert(
                                    BlockPos { x, y, z },
                                    BlockSnapshot {
                                        state,
                                        block_entity: (state == 10)
                                            .then_some(vec![(x + 2) as u8, (z + 2) as u8]),
                                    },
                                );
                            }
                        }
                    }
                    let original = world.blocks.clone();
                    let mut expected = original.clone();
                    for p in source.iter() {
                        expected.insert(
                            p,
                            BlockSnapshot {
                                state: 0,
                                block_entity: None,
                            },
                        );
                    }
                    for p in source.iter() {
                        expected.insert(p.checked_offset(offset).unwrap(), original[&p].clone());
                    }
                    let mut operation = MoveState::new(source, offset);
                    let mut history = Vec::new();
                    let mut finished = false;
                    for _ in 0..100 {
                        let result = operation.process(&mut world, &mut history, 2, 1000);
                        assert!(result.scanned() <= 2);
                        if result.is_finished() {
                            finished = true;
                            break;
                        }
                    }
                    assert!(finished);
                    assert_eq!(world.blocks, expected, "offset {offset:?}");
                    for change in history.iter().rev() {
                        replay_change(&mut world, change, ReplayDirection::Undo);
                    }
                    assert_eq!(world.blocks, original, "undo offset {offset:?}");
                    for change in &history {
                        replay_change(&mut world, change, ReplayDirection::Redo);
                    }
                    assert_eq!(world.blocks, expected, "redo offset {offset:?}");
                }
            }
        }
    }
}
