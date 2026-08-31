use coln_store::{
    commit::hash::CommitHash,
    table::{CellValue, RowId},
    txn::{RowRef, TempRowId, TxnCellValue},
};
use std::u32;

pub struct EncodedTupleGeneric<S> {
    len: usize,
    tags: S,
    values: S,
    storage: S,
    free_start: usize,
}

const STRING_TAG: u8 = 0;
const INT_TAG: u8 = 1;
const EXISTING_ROW_ID_TAG: u8 = 2;
const INPROGRESS_ROW_ID_TAG: u8 = 3;

const HASH_LEN: usize = 32;

pub type EncodedTuple<'a> = EncodedTupleGeneric<&'a [u8]>;
pub type EncodedTupleMut<'a> = EncodedTupleGeneric<&'a mut [u8]>;

impl<'a> EncodedTupleGeneric<&'a [u8]> {
    pub fn read(bytes: &'a [u8]) -> Self {
        let len = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        Self {
            len,
            tags: &bytes[4..4 + len],
            values: &bytes[4 + len..4 + 9 * len],
            storage: &bytes[4 + 9 * len..],
            free_start: bytes.len() - 4 + 9 * len,
        }
    }
}

impl<'a> EncodedTupleGeneric<&'a mut [u8]> {
    pub fn size_for(len: usize, storage_size: usize) -> usize {
        4 + 9 * len + storage_size
    }

    pub fn write(len: usize, bytes: &'a mut [u8]) -> Self {
        let (len_bytes, rest) = bytes.split_at_mut(4);
        len_bytes.copy_from_slice(&(len as u32).to_le_bytes()[..]);
        let (tags, rest) = rest.split_at_mut(len);
        let (values, storage) = rest.split_at_mut(len * 8);
        Self {
            len,
            tags,
            values,
            storage,
            free_start: 0,
        }
    }
}

impl<S: AsRef<[u8]>> EncodedTupleGeneric<S> {
    fn read_value_32_0(&self, i: usize) -> u32 {
        u32::from_le_bytes(self.values.as_ref()[8 * i..8 * i + 4].try_into().unwrap())
    }

    fn read_value_32_1(&self, i: usize) -> u32 {
        u32::from_le_bytes(
            self.values.as_ref()[8 * i + 4..8 * i + 4]
                .try_into()
                .unwrap(),
        )
    }

    fn read_string(&self, i: usize) -> String {
        let start = self.read_value_32_0(i) as usize;
        let len = self.read_value_32_1(i) as usize;
        String::from_utf8(self.storage.as_ref()[start..start + len].into()).unwrap()
    }

    fn read_int(&self, i: usize) -> i64 {
        i64::from_le_bytes(self.values.as_ref()[8 * i..8 * i + 8].try_into().unwrap())
    }

    fn read_existing_row_id(&self, i: usize) -> RowId {
        let start = self.read_value_32_0(i) as usize;
        RowId {
            commit: CommitHash(
                self.storage.as_ref()[start..start + HASH_LEN]
                    .try_into()
                    .unwrap(),
            ),
            counter: self.read_value_32_1(i),
        }
    }

    fn read_pending_row_id(&self, i: usize) -> TempRowId {
        self.read_value_32_1(i).into()
    }

    pub fn read_cell_value(&self, i: usize) -> CellValue {
        assert!(i < self.len);
        match self.tags.as_ref()[i] {
            STRING_TAG => CellValue::Str(self.read_string(i)),
            INT_TAG => CellValue::Int(self.read_int(i)),
            EXISTING_ROW_ID_TAG => CellValue::Id(self.read_existing_row_id(i)),
            INPROGRESS_ROW_ID_TAG => {
                panic!("in-progress row id used outside of a transaction")
            }
            _ => {
                panic!("unknown tag")
            }
        }
    }

    pub fn read_txn_cell_value(&self, i: usize) -> TxnCellValue {
        assert!(i < self.len);
        match self.tags.as_ref()[i] {
            STRING_TAG => TxnCellValue::Str(self.read_string(i)),
            INT_TAG => TxnCellValue::Int(self.read_int(i)),
            EXISTING_ROW_ID_TAG => TxnCellValue::Id(RowRef::Existing(self.read_existing_row_id(i))),
            INPROGRESS_ROW_ID_TAG => TxnCellValue::Id(RowRef::Pending(self.read_pending_row_id(i))),
            _ => {
                panic!("unknown tag")
            }
        }
    }
}

impl<S: AsMut<[u8]>> EncodedTupleGeneric<S> {
    fn write_value_32_0(&mut self, i: usize, v: u32) {
        self.values.as_mut()[i * 8..i * 8 + 4].copy_from_slice(&u32::to_le_bytes(v));
    }

    fn write_value_32_1(&mut self, i: usize, v: u32) {
        self.values.as_mut()[i * 8..i * 8 + 4].copy_from_slice(&u32::to_le_bytes(v));
    }

    fn write_int(&mut self, i: usize, v: i64) {
        self.tags.as_mut()[i] = INT_TAG;
        self.values.as_mut()[i * 8..i * 8 + 8].copy_from_slice(&i64::to_le_bytes(v));
    }

    fn write_string(&mut self, i: usize, v: &str) {
        let start = self.free_start;
        let len = v.len();
        assert!(start + len < self.storage.as_mut().len());
        self.storage.as_mut()[start..start + len].copy_from_slice(v.as_bytes());
        self.tags.as_mut()[i] = STRING_TAG;
        self.write_value_32_0(i, start as u32);
        self.write_value_32_1(i, len as u32);
        self.free_start += len;
    }

    fn write_row_id(&mut self, i: usize, v: RowId) {
        let start = self.free_start;
        let len = HASH_LEN;
        assert!(start + len < self.storage.as_mut().len());
        self.storage.as_mut()[start..start + len].copy_from_slice(&v.commit.0[..]);
        self.tags.as_mut()[i] = EXISTING_ROW_ID_TAG;
        self.write_value_32_0(i, start as u32);
        self.write_value_32_1(i, v.counter as u32);
        self.free_start += len;
    }

    fn write_pending_row_id(&mut self, i: usize, v: TempRowId) {
        self.tags.as_mut()[i] = INPROGRESS_ROW_ID_TAG;
        self.write_value_32_0(i, 0);
        self.write_value_32_1(i, v.0 as u32);
    }
}
