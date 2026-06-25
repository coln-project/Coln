#![allow(unused)]

use std::collections::HashMap;

use crate::commit::leb128 as commit_leb128;
use crate::commit::wire::prim;
use crate::roweq::ObservedOutcome;
use crate::table::CellValue;
use crate::table::RowId;
use crate::table::Table;

const HASH_SIZE: usize = 16;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct ContentHash([u8; HASH_SIZE]);

impl ContentHash {
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    fn from_hasher(hasher: blake3::Hasher) -> Self {
        let digest = hasher.finalize();
        Self(digest.as_bytes()[..HASH_SIZE].try_into().unwrap())
    }
}

enum Tag {
    Int,
    Str,
    LeafId,
    InternalId,
    EntireRow,
}

impl Tag {
    fn num(&self) -> u8 {
        match self {
            Tag::Int => 0x01,
            Tag::Str => 0x02,
            Tag::LeafId => 0x03,
            Tag::InternalId => 0x04,
            Tag::EntireRow => 0x10,
        }
    }
}

struct ContentHasher {
    inner: blake3::Hasher,
}

impl ContentHasher {
    fn new() -> Self {
        Self {
            inner: blake3::Hasher::new(),
        }
    }

    fn tag(&mut self, tag: Tag) {
        self.inner.update(&[tag.num()]);
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.inner.update(bytes);
    }

    fn row_id(&mut self, rid: RowId) {
        self.bytes(rid.commit.as_bytes());
        let mut buf = Vec::new();
        commit_leb128::write_u32(&mut buf, rid.counter);
        self.bytes(&buf);
    }

    fn content_hash(&mut self, hash: ContentHash) {
        self.bytes(hash.as_bytes());
    }

    fn finish(self) -> ContentHash {
        ContentHash::from_hasher(self.inner)
    }
}

pub(crate) struct Index {
    by_row: HashMap<RowId, ContentHash>,
    canonical: HashMap<ContentHash, RowId>,
}

#[derive(Debug, thiserror::Error)]
pub enum HashDagError {
    #[error("child hash missing when constructing parent hash")]
    MissingChildHash { row_id: RowId },
}

impl Index {
    /// Given a `row_id` returns the canonical `row_id` for it. Two rows map to
    /// the same id when they contain the same hash, and the canonical id is the
    /// lexical smaller one
    pub fn canonical(&self, row_id: &RowId) -> RowId {
        *self
            .by_row
            .get(row_id)
            .and_then(|h| self.canonical.get(h))
            .unwrap_or(row_id)
    }

    pub fn hash_row_id(&self, rid: RowId) -> ContentHash {
        let mut hasher = ContentHasher::new();
        hasher.tag(Tag::LeafId);
        hasher.row_id(rid);
        hasher.finish()
    }

    pub(crate) fn hash_cell(&self, val: &CellValue) -> Result<ContentHash, HashDagError> {
        let mut hasher = ContentHasher::new();
        match val {
            CellValue::Id(rid) => {
                let h = self
                    .by_row
                    .get(rid)
                    .ok_or(HashDagError::MissingChildHash { row_id: *rid })?;

                hasher.tag(Tag::InternalId);
                hasher.content_hash(*h);
            }
            CellValue::Int(i) => {
                hasher.tag(Tag::Int);
                let mut buf = Vec::new();
                commit_leb128::write_i64(&mut buf, *i);
                hasher.bytes(&buf);
            }
            CellValue::Str(s) => {
                hasher.tag(Tag::Str);
                hasher.bytes(s.as_bytes());
            }
        }

        Ok(hasher.finish())
    }

    pub(crate) fn hash_row(
        &self,
        table: &Table,
        rid: RowId,
        vals: &[CellValue],
    ) -> Result<ContentHash, HashDagError> {
        let mut hasher = ContentHasher::new();
        hasher.tag(Tag::EntireRow);
        hasher.bytes(&prim::encode_path(table.path()));

        if !table.hashcons() {
            let h = self.hash_row_id(rid);
            hasher.content_hash(h);
        }
        for v in vals {
            let h = self.hash_cell(v)?;
            hasher.content_hash(h);
        }
        Ok(hasher.finish())
    }

    pub(crate) fn observe(&mut self, table: &Table, rid: RowId, h: ContentHash) -> ObservedOutcome {
        if let Some(existing) = self.by_row.get(&rid) {
            debug_assert_eq!(
                existing, &h,
                "row id was observed with a different content hash"
            );
        }
        self.by_row.insert(rid, h);

        if !table.hashcons() {
            // no need to dedup, this table is not a hashcons table
            return ObservedOutcome::Inserted(rid);
        }

        match self.canonical.get(&h) {
            None => {
                self.canonical.insert(h, rid);
                ObservedOutcome::Inserted(rid)
            }
            Some(&old) if rid < old => {
                self.canonical.insert(h, rid);
                ObservedOutcome::Swap { old, new: rid }
            }
            Some(&old) if old == rid => ObservedOutcome::KeptOld(rid),
            Some(&old) => ObservedOutcome::KeptOld(old),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::hash::CommitHash;
    use crate::ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, Schema};

    fn row_id(commit_byte: u8, counter: u32) -> RowId {
        RowId {
            commit: CommitHash([commit_byte; 32]),
            counter,
        }
    }

    fn empty_index() -> Index {
        Index {
            by_row: HashMap::new(),
            canonical: HashMap::new(),
        }
    }

    fn int_table(path: &str, hashcons: bool) -> Table {
        let schema = Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![ColumnEntry {
                path: Path::from("value"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            }],
            primary_key: None,
        };
        let mut table = Table::new(Path::from(path), schema);
        table.set_hashcons_for_test(hashcons);
        table
    }

    #[test]
    fn hash_row_id_distinguishes_row_ids() {
        let index = empty_index();
        let first = row_id(1, 0);
        let same = row_id(1, 0);
        let different_counter = row_id(1, 1);
        let different_commit = row_id(2, 0);

        assert_eq!(index.hash_row_id(first), index.hash_row_id(same));
        assert_ne!(
            index.hash_row_id(first),
            index.hash_row_id(different_counter)
        );
        assert_ne!(
            index.hash_row_id(first),
            index.hash_row_id(different_commit)
        );
    }

    #[test]
    fn hash_cell_missing_child_hash_returns_error() {
        let index = empty_index();
        let missing = row_id(1, 0);

        let err = index
            .hash_cell(&CellValue::Id(missing))
            .expect_err("missing child hash should be reported");

        assert!(matches!(
            err,
            HashDagError::MissingChildHash { row_id } if row_id == missing
        ));
    }

    #[test]
    fn hash_cell_id_uses_child_content_hash() {
        let mut index = empty_index();
        let child = row_id(1, 0);
        let child_hash = index.hash_row_id(child);
        index.by_row.insert(child, child_hash);

        let id_hash = index
            .hash_cell(&CellValue::Id(child))
            .expect("child hash is present");

        assert_ne!(id_hash, index.hash_row_id(child));
        assert_eq!(
            id_hash,
            index
                .hash_cell(&CellValue::Id(child))
                .expect("child hash is still present")
        );
    }

    #[test]
    fn hash_row_non_hashcons_includes_own_row_id() {
        let index = empty_index();
        let table = int_table("T", false);
        let values = vec![CellValue::Int(7)];

        let first = index
            .hash_row(&table, row_id(1, 0), &values)
            .expect("hash first row");
        let same = index
            .hash_row(&table, row_id(1, 0), &values)
            .expect("hash same row");
        let different = index
            .hash_row(&table, row_id(1, 1), &values)
            .expect("hash different row");

        assert_eq!(first, same);
        assert_ne!(first, different);
    }

    #[test]
    fn hash_row_hashcons_excludes_own_row_id() {
        let index = empty_index();
        let table = int_table("T", true);
        let values = vec![CellValue::Int(7)];

        let first = index
            .hash_row(&table, row_id(1, 0), &values)
            .expect("hash first row");
        let same_content = index
            .hash_row(&table, row_id(1, 1), &values)
            .expect("hash same content");
        let different_content = index
            .hash_row(&table, row_id(1, 1), &[CellValue::Int(8)])
            .expect("hash different content");

        assert_eq!(first, same_content);
        assert_ne!(first, different_content);
    }
}
