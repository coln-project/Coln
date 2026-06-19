use std::io::Write;

use coln_lang_rs::ir::BuiltinTy;
use hexane::{PackError, lebsize};

use crate::{commit::error::CodecError, txn::ops::TxnCellValue};

/// Number of low bits reserved for the [`ValueType`] code in a [`ValueMeta`].
const TYPE_CODE_BITS: u32 = 5;
/// Mask selecting the [`ValueType`] code.
const TYPE_CODE_MASK: u8 = (1 << TYPE_CODE_BITS) - 1;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ValueType {
    Null,
    False,
    True,
    Uleb,
    Leb,
    BigInt,
    BigUint,
    BigRat,
    BFloat16,
    Float16,
    Float32,
    Float64,
    String,
    Bytes,
    Unknown(u8),
}

impl ValueType {
    /// The 5-bit type code stored in the low bit of a [`ValueMeta`].
    pub(crate) fn code(self) -> u8 {
        match self {
            ValueType::Null => 0,
            ValueType::False => 1,
            ValueType::True => 2,
            ValueType::Uleb => 3,
            ValueType::Leb => 4,
            ValueType::BigInt => 5,
            ValueType::BigUint => 6,
            ValueType::BigRat => 7,
            ValueType::Float16 => 8,
            ValueType::Float32 => 9,
            ValueType::Float64 => 10,
            ValueType::BFloat16 => 11,
            ValueType::String => 12,
            ValueType::Bytes => 13,
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
            5 => ValueType::BigInt,
            6 => ValueType::BigUint,
            7 => ValueType::BigRat,
            8 => ValueType::Float16,
            9 => ValueType::Float32,
            10 => ValueType::Float64,
            11 => ValueType::BFloat16,
            12 => ValueType::String,
            13 => ValueType::Bytes,
            other => ValueType::Unknown(other),
        }
    }

    /// Whether this physical representation is permitted in a column whose
    /// logical (schema) type is `prim`. The schema is the contract: it bounds
    /// which representations may appear, while the per-value type code records
    /// which one was actually used.
    pub(crate) fn is_valid_for(self, prim: &BuiltinTy) -> bool {
        match prim {
            BuiltinTy::BuiltinInt => {
                matches!(
                    self,
                    ValueType::Uleb | ValueType::Leb | ValueType::BigInt | ValueType::BigUint
                )
            }
            BuiltinTy::BuiltinStr => matches!(self, ValueType::String),
        }
    }
}

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

pub(crate) fn encode_prim_value(
    value: &TxnCellValue,
    prim: &BuiltinTy,
    out: &mut Vec<u8>,
) -> Result<ValueMeta, CodecError> {
    match prim {
        BuiltinTy::BuiltinInt => {
            let TxnCellValue::Int(i) = value else {
                return Err(CodecError::SchemaError(format!(
                    "expected int, got {value:?}"
                )));
            };

            leb128::write::signed(out, *i)
                .map_err(|e| CodecError::DataFormatError(e.to_string()))?;

            Ok(ValueMeta::new(ValueType::Leb, lebsize(*i) as usize))
        }
        BuiltinTy::BuiltinStr => {
            let TxnCellValue::Str(s) = value else {
                return Err(CodecError::SchemaError(format!(
                    "expected string, got {value:?}"
                )));
            };
            // raw utf-8 bytes; length here MUST equal ValueMeta::length()
            out.extend_from_slice(s.as_bytes());

            Ok(ValueMeta::new(ValueType::String, s.len()))
        }
    }
}

pub(crate) fn decode_prim_value(
    meta: ValueMeta,
    prim: &BuiltinTy,
    bytes: &[u8],
) -> Result<TxnCellValue, CodecError> {
    let ty = meta.type_code();
    if !ty.is_valid_for(prim) {
        return Err(CodecError::SchemaError(format!(
            "value type {ty:?} is not valid for column type {prim:?}"
        )));
    }

    match ty {
        ValueType::Leb => {
            let mut reader = bytes;
            let i = leb128::read::signed(&mut reader)
                .map_err(|e| CodecError::DataFormatError(e.to_string()))?;
            if !reader.is_empty() {
                return Err(CodecError::DataFormatError(
                    "trailing bytes in leb value".into(),
                ));
            }
            Ok(TxnCellValue::Int(i))
        }
        ValueType::String => {
            let s = std::str::from_utf8(bytes)
                .map_err(|_| CodecError::DataFormatError("value column: invalid utf-8".into()))?;
            Ok(TxnCellValue::Str(s.to_owned()))
        }
        other => Err(CodecError::DataFormatError(format!(
            "unsupported value type code {other:?}"
        ))),
    }
}

#[allow(dead_code)]
fn encode_bigint(value: &[u8], out: &mut Vec<u8>) -> Result<ValueMeta, CodecError> {
    out.write_all(value)?;
    Ok(ValueMeta::new(ValueType::BigInt, value.len()))
}

#[allow(dead_code)]
fn encode_bigrat(_value: &[u8], _out: &mut Vec<u8>) -> Result<ValueMeta, CodecError> {
    // canonicalize: denominator positive, reduced fraction, zero as 0/1
    // encode numerator with BigInt canonical bytes
    // encode denominator with BigUint canonical bytes
    // write numerator_len, then numerator bytes, then denominator bytes
    // TODO implement when IR supports it
    todo!()
}
