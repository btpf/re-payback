//! A read-only classic HFS reader, enough for `PaybackMac.toast` (a bare
//! HFS volume, the Master Directory Block at byte 1024): the catalog B-tree
//! for the folder tree and the extents overflow B-tree for fragmented
//! files. Data forks only (the port reads no resource forks).

use std::collections::HashMap;
use std::io;

fn be16(b: &[u8], at: usize) -> usize {
    u16::from_be_bytes([b[at], b[at + 1]]) as usize
}

fn be32(b: &[u8], at: usize) -> usize {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as usize
}

/// Three (start block, block count) extents.
fn extents(b: &[u8], at: usize) -> [(usize, usize); 3] {
    [0, 1, 2].map(|i| (be16(b, at + 4 * i), be16(b, at + 4 * i + 2)))
}

/// Mac OS Roman, 0x80..=0xFF, as Python's `mac_roman` codec decodes it.
const MAC_ROMAN: &str = "ÄÅÇÉÑÖÜáàâäãåçéèêëíìîïñóòôöõúùûü†°¢£§•¶ß®©™´¨≠ÆØ∞±≤≥¥µ∂∑∏π∫ªºΩæø\
¿¡¬√ƒ≈∆«»…\u{a0}ÀÃÕŒœ–—“”‘’÷◊ÿŸ⁄€‹›ﬁﬂ‡·‚„‰ÂÊÁËÈÍÎÏÌÓÔ\u{f8ff}ÒÚÛÙıˆ˜¯˘˙˚¸˝˛ˇ";

fn mac_roman(b: &[u8]) -> String {
    b.iter()
        .map(|&c| if c < 0x80 { c as char } else { MAC_ROMAN.chars().nth(c as usize - 0x80).unwrap_or('_') })
        .collect()
}

pub struct Volume<'a> {
    img: &'a [u8],
    block: usize,
    first: usize,
    /// Extents overflow records: (fork type, file id) → (start block, extents).
    overflow: HashMap<(u8, usize), Vec<(usize, [(usize, usize); 3])>>,
}

struct Entry {
    parent: usize,
    name: String,
    /// Some for a file: id, logical length, first extents.
    file: Option<(usize, usize, [(usize, usize); 3])>,
    dir_id: usize,
}

impl<'a> Volume<'a> {
    pub fn open(img: &'a [u8]) -> io::Result<Volume<'a>> {
        let bad = || io::Error::other("not an HFS volume");
        let mdb = img.get(1024..1024 + 162).ok_or_else(bad)?;
        if &mdb[0..2] != b"BD" {
            return Err(bad());
        }
        let mut v = Volume { img, block: be32(mdb, 20), first: be16(mdb, 28) * 512, overflow: HashMap::new() };
        let xt = v.fork(3, 0, extents(mdb, 134), be32(mdb, 130))?;
        let mut overflow: HashMap<_, Vec<_>> = HashMap::new();
        for rec in leaf_records(&xt)? {
            if rec.len() >= 20 && rec[0] >= 7 {
                overflow.entry((rec[1], be32(rec, 2))).or_default().push((be16(rec, 6), extents(rec, 8)));
            }
        }
        for v in overflow.values_mut() {
            v.sort_by_key(|e| e.0);
        }
        v.overflow = overflow;
        Ok(v)
    }

    /// A fork's bytes: its first three extents, then its overflow records.
    fn fork(&self, id: usize, kind: u8, first: [(usize, usize); 3], len: usize) -> io::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(len);
        let more = self.overflow.get(&(kind, id)).into_iter().flatten().flat_map(|(_, e)| e.iter());
        for &(start, count) in first.iter().chain(more) {
            if out.len() >= len {
                break;
            }
            let at = self.first + start * self.block;
            let n = (count * self.block).min(len - out.len());
            out.extend_from_slice(self.img.get(at..at + n).ok_or_else(|| io::Error::other("extent past the image"))?);
        }
        if out.len() < len {
            return Err(io::Error::other(format!("file {id}: fork shorter than its length")));
        }
        Ok(out)
    }

    /// Calls `f` with the `/`-separated path (below the volume's root
    /// folder) and the data fork of every file. Names are decoded from Mac
    /// OS Roman; `name` maps each path component (e.g. for Windows).
    pub fn walk(&self, name: &dyn Fn(&str) -> String, f: &mut dyn FnMut(&str, Vec<u8>) -> io::Result<()>) -> io::Result<()> {
        let mdb = &self.img[1024..];
        let cat = self.fork(4, 0, extents(mdb, 150), be32(mdb, 146))?;
        let mut entries = Vec::new();
        for rec in leaf_records(&cat)? {
            let kl = rec[0] as usize;
            if kl < 6 || rec.len() < 8 {
                continue;
            }
            let parent = be32(rec, 2);
            let nl = rec[6] as usize;
            let nm = mac_roman(&rec[7..(7 + nl).min(rec.len())]);
            let d = &rec[(1 + kl + 1) & !1..];
            match d.first() {
                Some(1) if d.len() >= 10 => entries.push(Entry { parent, name: nm, file: None, dir_id: be32(d, 6) }),
                Some(2) if d.len() >= 86 => {
                    entries.push(Entry { parent, name: nm, file: Some((be32(d, 20), be32(d, 26), extents(d, 74))), dir_id: 0 })
                }
                _ => {}
            }
        }
        let dirs: HashMap<usize, (usize, String)> =
            entries.iter().filter(|e| e.file.is_none()).map(|e| (e.dir_id, (e.parent, name(&e.name)))).collect();
        for e in &entries {
            let Some((id, len, first)) = e.file else { continue };
            // Up to the root folder (id 2), whose name is the volume's.
            let mut parts = vec![name(&e.name)];
            let mut dir = e.parent;
            while dir > 2 {
                let Some((up, n)) = dirs.get(&dir) else { break };
                parts.push(n.clone());
                dir = *up;
            }
            parts.reverse();
            f(&parts.join("/"), self.fork(id, 0, first, len)?)?;
        }
        Ok(())
    }
}

/// The records of every leaf node of a B-tree file, first leaf to last.
fn leaf_records(tree: &[u8]) -> io::Result<Vec<&[u8]>> {
    if tree.len() < 512 {
        return Ok(Vec::new());
    }
    let ns = be16(tree, 14 + 18);
    let mut node = be32(tree, 14 + 10);
    let mut out = Vec::new();
    let mut seen = 0;
    while node != 0 {
        seen += 1;
        let nd = tree.get(node * ns..node * ns + ns).ok_or_else(|| io::Error::other("B-tree node past the file"))?;
        if seen > tree.len() / ns {
            return Err(io::Error::other("B-tree leaf chain loops"));
        }
        let n = be16(nd, 10);
        for r in 0..n {
            let off = be16(nd, ns - 2 - 2 * r);
            let end = be16(nd, ns - 4 - 2 * r);
            if off < end && end <= ns {
                out.push(&nd[off..end]);
            }
        }
        node = be32(nd, 0);
    }
    Ok(out)
}
