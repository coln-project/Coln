use coln_lang_rs::ir::PrimType;
use hexane::{PackError, lebsize};

/// follows ir::PrimType, but contains an actual value
pub(crate) enum PrimValue<'a> {
    Int(i64),
    Str(&'a str),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ValueType {
    Null,
    False,
    True,
    Uleb,
    Leb,
    // need different precision
    Float,
    String,
    Bytes,
    // TODO arbitrary precision int, rational, bfloat
    Unknown(u8),
}

impl ValueType {
    /// The 4-bit type code stored in the low nibble of a [`ValueMeta`].
    pub(crate) fn code(self) -> u8 {
        match self {
            ValueType::Null => 0,
            ValueType::False => 1,
            ValueType::True => 2,
            ValueType::Uleb => 3,
            ValueType::Leb => 4,
            ValueType::Float => 5,
            ValueType::String => 6,
            ValueType::Bytes => 7,
            ValueType::Unknown(code) => code,
        }
    }

    /// Inverse of [`ValueType::code`]; codes outside the known range decode
    /// to [`ValueType::Unknown`] for forward compatibility.
    fn from_code(code: u8) -> Self {
        match code {
            0 => ValueType::Null,
            1 => ValueType::False,
            2 => ValueType::True,
            3 => ValueType::Uleb,
            4 => ValueType::Leb,
            5 => ValueType::Float,
            6 => ValueType::String,
            7 => ValueType::Bytes,
            other => ValueType::Unknown(other),
        }
    }

    /// Whether this physical representation is permitted in a column whose
    /// logical (schema) type is `prim`. The schema is the contract: it bounds
    /// which representations may appear, while the per-value type code records
    /// which one was actually used.
    pub(crate) fn is_valid_for(self, prim: &PrimType) -> bool {
        match prim {
            PrimType::PrimInt => matches!(self, ValueType::Uleb | ValueType::Leb),
            PrimType::PrimString => matches!(self, ValueType::String),
        }
    }
}

/// Number of low bits reserved for the [`ValueType`] code in a [`ValueMeta`].
const TYPE_CODE_BITS: u32 = 4;
/// Mask selecting the [`ValueType`] code nibble.
const TYPE_CODE_MASK: u8 = (1 << TYPE_CODE_BITS) - 1;

#[derive(Copy, Clone, Debug, Default, PartialEq, PartialOrd)]
pub(crate) struct ValueMeta(u64);

impl ValueMeta {
    /// Pack a type code and byte length into the meta word: the low
    /// [`TYPE_CODE_BITS`] hold the [`ValueType`] code, the remaining bits hold
    /// the value's byte length.
    pub(crate) fn new(ty: ValueType, length: usize) -> Self {
        Self(((length as u64) << TYPE_CODE_BITS) | ty.code() as u64)
    }

    pub(crate) fn type_code(&self) -> ValueType {
        ValueType::from_code((self.0 as u8) & TYPE_CODE_MASK)
    }

    pub(crate) fn length(&self) -> usize {
        (self.0 >> TYPE_CODE_BITS) as usize
    }
}

impl From<&PrimValue<'_>> for ValueMeta {
    fn from(p: &PrimValue<'_>) -> Self {
        match p {
            PrimValue::Int(i) => ValueMeta::new(ValueType::Leb, lebsize(*i) as usize),
            PrimValue::Str(s) => ValueMeta::new(ValueType::String, s.len()),
        }
    }
}

impl hexane::v1::ColumnValue for ValueMeta {
    type Encoding = hexane::v1::RleEncoding<ValueMeta>;
}

impl hexane::v1::RleValue for ValueMeta {
    fn try_unpack(data: &[u8]) -> Result<(usize, ValueMeta), PackError> {
        let mut buf = data;
        let start = buf.len();
        let v = leb128::read::unsigned(&mut buf)?;
        Ok((start - buf.len(), ValueMeta(v)))
    }
    fn pack(value: ValueMeta, out: &mut Vec<u8>) -> bool {
        leb128::write::unsigned(out, value.0).unwrap();
        true
    }
}

impl hexane::v1::PrefixValue for ValueMeta {
    type Prefix = u64;
    fn accumulate(target: &mut u64, val: ValueMeta) {
        *target += val.length() as u64;
    }
    fn accumulate_run(target: &mut u64, run: &hexane::v1::Run<ValueMeta>) {
        *target += run.value.length() as u64 * run.count as u64;
    }
}
