//! Transposition table (§4.2 item 4). Filled in by task T2.2.

use crate::types::Move;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bound {
    Exact,
    Lower,
    Upper,
}

#[derive(Clone, Copy, Debug)]
pub struct TtEntry {
    pub key: u64,
    pub mv: Option<Move>,
    pub score: i16,
    pub depth: u8,
    pub bound: Bound,
    pub age: u8,
}

/// Power-of-two sized transposition table.
pub struct TranspositionTable {
    entries: Vec<Option<TtEntry>>,
    mask: usize,
    pub age: u8,
}

impl TranspositionTable {
    #[must_use]
    pub fn new(mb: usize) -> Self {
        let bytes = mb.max(1) * 1024 * 1024;
        let mut cap = 1usize;
        while cap * std::mem::size_of::<TtEntry>() * 2 <= bytes {
            cap *= 2;
        }
        Self {
            entries: vec![None; cap],
            mask: cap - 1,
            age: 0,
        }
    }

    pub fn store(&mut self, key: u64, mv: Option<Move>, score: i16, depth: u8, bound: Bound) {
        let idx = (key as usize) & self.mask;
        let replace = match &self.entries[idx] {
            None => true,
            Some(e) => depth >= e.depth || e.age != self.age,
        };
        if replace {
            self.entries[idx] = Some(TtEntry {
                key,
                mv,
                score,
                depth,
                bound,
                age: self.age,
            });
        }
    }

    #[must_use]
    pub fn probe(&self, key: u64) -> Option<TtEntry> {
        let idx = (key as usize) & self.mask;
        self.entries[idx].filter(|e| e.key == key)
    }

    pub fn clear(&mut self) {
        self.entries.iter_mut().for_each(|e| *e = None);
        self.age = self.age.wrapping_add(1);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.iter().filter(|e| e.is_some()).count()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
