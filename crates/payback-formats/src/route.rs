//! `Maps/<n>.route` — the police routing graph (docs/re/combat_police.md §1).
//!
//! 1024 node records × 270 bytes, big-endian:
//!
//! ```text
//! u16 link[4]        target node index, 9999 = none (links are directed)
//! u16 x, y           map units, 256 per square (object coords = << 3)
//! u8  used           0xFF = unused slot
//! u8  next_hop[256]  2 bits per destination node: which link to follow
//! u8  pad
//! ```

use crate::{Error, Result};

pub const NODES: usize = 1024;
const RECORD: usize = 270;
pub const NO_LINK: u16 = 9999;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub links: [u16; 4],
    pub x: u16,
    pub y: u16,
    pub used: bool,
    next_hop: [u8; 256],
}

impl Node {
    /// Outgoing links (skipping empty slots).
    pub fn targets(&self) -> impl Iterator<Item = usize> + '_ {
        self.links
            .iter()
            .filter(|&&l| l != NO_LINK)
            .map(|&l| l as usize)
    }

    /// Link slot (0..3) to follow toward destination node `dest`.
    pub fn hop_slot(&self, dest: usize) -> usize {
        ((self.next_hop[dest >> 2] >> ((dest & 3) * 2)) & 3) as usize
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub nodes: Vec<Node>,
}

impl Route {
    pub fn parse(data: &[u8]) -> Result<Route> {
        if data.len() < NODES * RECORD {
            return Err(Error::Truncated("route"));
        }
        let nodes = data
            .chunks_exact(RECORD)
            .take(NODES)
            .map(|r| {
                let u16_at = |o: usize| u16::from_be_bytes([r[o], r[o + 1]]);
                let mut next_hop = [0u8; 256];
                next_hop.copy_from_slice(&r[13..269]);
                Node {
                    links: [u16_at(0), u16_at(2), u16_at(4), u16_at(6)],
                    x: u16_at(8),
                    y: u16_at(10),
                    used: r[12] != 0xFF,
                    next_hop,
                }
            })
            .collect();
        Ok(Route { nodes })
    }

    /// A graph with no nodes (missing file: police drive "erratically").
    pub fn empty() -> Route {
        Route { nodes: Vec::new() }
    }

    /// Next node from `node` toward `dest` (68k `FUN_000a43f2`).
    pub fn next(&self, node: usize, dest: usize) -> Option<usize> {
        let n = self.nodes.get(node)?;
        let l = n.links[n.hop_slot(dest)];
        (l != NO_LINK).then_some(l as usize)
    }

    /// Nearest node to (x, y) in map units (68k `FUN_000a42ac`): each used
    /// node competes by squared distance, and so do points sampled every 512
    /// map units along its outgoing links, which select the link's target.
    pub fn nearest(&self, x: i32, y: i32) -> Option<usize> {
        let mut best = (i64::MAX, None);
        let dist = |px: i32, py: i32| {
            let (dx, dy) = ((px - x) as i64, (py - y) as i64);
            dx * dx + dy * dy
        };
        for (i, n) in self.nodes.iter().enumerate().filter(|(_, n)| n.used) {
            let (nx, ny) = (n.x as i32, n.y as i32);
            let d = dist(nx, ny);
            if d < best.0 {
                best = (d, Some(i));
            }
            for t in n.targets() {
                let Some(tn) = self.nodes.get(t) else {
                    continue;
                };
                let (tx, ty) = (tn.x as i32, tn.y as i32);
                let steps = ((tx - nx).abs() + (ty - ny).abs()) >> 9;
                for k in 1..steps {
                    let sx = nx + (tx - nx) * k / steps;
                    let sy = ny + (ty - ny) * k / steps;
                    let d = dist(sx, sy);
                    if d < best.0 {
                        best = (d, Some(t));
                    }
                }
            }
        }
        best.1
    }

    /// Farthest used node from (x, y) (68k `FUN_000a430a`).
    pub fn farthest(&self, x: i32, y: i32) -> Option<usize> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.used)
            .max_by_key(|(_, n)| {
                let (dx, dy) = ((n.x as i32 - x) as i64, (n.y as i32 - y) as i64);
                dx * dx + dy * dy
            })
            .map(|(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two nodes linked both ways: 0 at (256, 256), 1 at (2304, 256).
    fn two_nodes() -> Vec<u8> {
        let mut d = vec![0u8; NODES * RECORD];
        for i in 0..NODES {
            let r = &mut d[i * RECORD..(i + 1) * RECORD];
            for s in 0..4 {
                r[s * 2..s * 2 + 2].copy_from_slice(&NO_LINK.to_be_bytes());
            }
            r[12] = 0xFF;
        }
        let set = |d: &mut Vec<u8>, i: usize, link: u16, x: u16| {
            let r = &mut d[i * RECORD..(i + 1) * RECORD];
            r[0..2].copy_from_slice(&link.to_be_bytes());
            r[8..10].copy_from_slice(&x.to_be_bytes());
            r[10..12].copy_from_slice(&256u16.to_be_bytes());
            r[12] = 0;
        };
        set(&mut d, 0, 1, 256);
        set(&mut d, 1, 0, 2304);
        d
    }

    #[test]
    fn parses_and_routes() {
        let r = Route::parse(&two_nodes()).unwrap();
        assert!(r.nodes[0].used && r.nodes[1].used && !r.nodes[2].used);
        assert_eq!(r.next(0, 1), Some(1));
        assert_eq!(r.nearest(300, 300), Some(0));
        // A point near the middle of the 0 -> 1 link samples toward node 1.
        assert_eq!(r.nearest(1700, 256), Some(1));
        assert_eq!(r.farthest(256, 256), Some(1));
    }
}
