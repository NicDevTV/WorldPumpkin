// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use pumpkin_plugin_api::common::BlockPos as WitBlockPos;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    /// Adds an offset to each coordinate, returning `None` if any coordinate overflows.
    pub(super) fn checked_offset(self, offset: Self) -> Option<Self> {
        Some(Self {
            x: self.x.checked_add(offset.x)?,
            y: self.y.checked_add(offset.y)?,
            z: self.z.checked_add(offset.z)?,
        })
    }

    pub(super) fn checked_sub(self, offset: Self) -> Option<Self> {
        Some(Self {
            x: self.x.checked_sub(offset.x)?,
            y: self.y.checked_sub(offset.y)?,
            z: self.z.checked_sub(offset.z)?,
        })
    }
}

impl From<WitBlockPos> for BlockPos {
    /// Copies Pumpkin's block coordinates into the plugin's position type.
    fn from(pos: WitBlockPos) -> Self {
        Self {
            x: pos.x,
            y: pos.y,
            z: pos.z,
        }
    }
}

impl From<BlockPos> for WitBlockPos {
    /// Copies the plugin's block coordinates into Pumpkin's position type.
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
    /// Returns the normalized inclusive cuboid, or `None` when either endpoint is unset.
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
    pub(super) fn contains(self, pos: BlockPos) -> bool {
        pos.x >= self.min.x
            && pos.x <= self.max.x
            && pos.y >= self.min.y
            && pos.y <= self.max.y
            && pos.z >= self.min.z
            && pos.z <= self.max.z
    }
    /// Normalizes two corners into an inclusive cuboid with ordered bounds on every axis.
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

    /// Counts all blocks in the inclusive cuboid, saturating at `u64::MAX` on overflow.
    pub fn volume(self) -> u64 {
        let [x, y, z] = self.dimensions();
        x.saturating_mul(y).saturating_mul(z)
    }

    /// Counts unique blocks on the vertical faces, saturating at `u64::MAX` on overflow.
    pub fn wall_volume(self) -> u64 {
        let [x, y, z] = self.dimensions();
        let perimeter = if x == 1 || z == 1 {
            x * z
        } else {
            2 * x + 2 * z - 4
        };
        perimeter.saturating_mul(y)
    }

    /// Iterates every included position with X advancing fastest, then Z, then Y.
    pub fn iter(self) -> CuboidIter {
        CuboidIter {
            cuboid: self,
            next: Some(self.min),
        }
    }

    /// Iterates vertical faces without allocating a list of wall positions.
    pub fn walls(self) -> WallIter {
        WallIter {
            cuboid: self,
            next: Some(self.min),
        }
    }

    #[cfg(test)]
    pub fn wall_positions(self) -> Vec<BlockPos> {
        self.walls().collect()
    }

    /// Returns the inclusive X, Y, and Z lengths using widened coordinate arithmetic.
    pub fn dimensions(self) -> [u64; 3] {
        [
            (i64::from(self.max.x) - i64::from(self.min.x) + 1) as u64,
            (i64::from(self.max.y) - i64::from(self.min.y) + 1) as u64,
            (i64::from(self.max.z) - i64::from(self.min.z) + 1) as u64,
        ]
    }

    /// Offsets both corners, returning `None` if any translated coordinate overflows.
    pub fn translated(self, offset: BlockPos) -> Option<Self> {
        Some(Self {
            min: self.min.checked_offset(offset)?,
            max: self.max.checked_offset(offset)?,
        })
    }
}

pub struct WallIter {
    cuboid: Cuboid,
    next: Option<BlockPos>,
}

impl Iterator for WallIter {
    type Item = BlockPos;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next?;
        let min = self.cuboid.min;
        let max = self.cuboid.max;
        self.next = if current.x < max.x && (current.z == min.z || current.z == max.z) {
            Some(BlockPos {
                x: current.x + 1,
                ..current
            })
        } else if current.x < max.x {
            Some(BlockPos {
                x: max.x,
                ..current
            })
        } else if current.z < max.z {
            Some(BlockPos {
                x: min.x,
                z: current.z + 1,
                ..current
            })
        } else if current.y < max.y {
            Some(BlockPos {
                x: min.x,
                y: current.y + 1,
                z: min.z,
            })
        } else {
            None
        };
        Some(current)
    }
}

pub struct CuboidIter {
    cuboid: Cuboid,
    next: Option<BlockPos>,
}

impl Iterator for CuboidIter {
    type Item = BlockPos;

    /// Yields the next included position in X, Z, Y order, or `None` after the final corner.
    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next?;
        self.next = advance_position(self.cuboid, current);
        Some(current)
    }
}

/// Advances within the inclusive cuboid in X, Z, Y order without stepping past its bounds.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn wall_iterator_matches_geometric_boundary_for_thin_and_regular_cuboids() {
        for x in 1..=5 {
            for y in 1..=4 {
                for z in 1..=5 {
                    let min = BlockPos {
                        x: -3,
                        y: -2,
                        z: -4,
                    };
                    let max = BlockPos {
                        x: min.x + x - 1,
                        y: min.y + y - 1,
                        z: min.z + z - 1,
                    };
                    let cuboid = Cuboid::new(max, min);
                    let expected: HashSet<_> = cuboid
                        .iter()
                        .filter(|p| p.x == min.x || p.x == max.x || p.z == min.z || p.z == max.z)
                        .collect();
                    let actual: Vec<_> = cuboid.walls().collect();
                    assert_eq!(actual.len() as u64, cuboid.wall_volume());
                    assert_eq!(actual.iter().copied().collect::<HashSet<_>>(), expected);
                    assert_eq!(actual.len(), expected.len());
                }
            }
        }
    }

    #[test]
    fn wall_iterator_handles_maximum_coordinates_without_overflow() {
        let cuboid = Cuboid::new(
            BlockPos {
                x: i32::MAX - 1,
                y: i32::MAX,
                z: i32::MAX - 1,
            },
            BlockPos {
                x: i32::MAX,
                y: i32::MAX,
                z: i32::MAX,
            },
        );
        assert_eq!(cuboid.walls().count(), 4);
    }
}
