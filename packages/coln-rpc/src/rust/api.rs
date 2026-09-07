// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir;
use serde::{Deserialize, Serialize};
use specta::Type;
// use coln_store::id_packer::IdPacker;
use coln_store::{table::WireValue, txn::TxnWireRowId};

#[derive(Type, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WhereClause {
    PrimaryKey { values: Vec<WireValue> },
    ExceptRowId { values: Vec<WireValue> },
    Generic { values_at: Vec<(u32, WireValue)> },
}

#[derive(Type, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SelectClause {
    All { columns: Vec<u32> },
    One { columns: Vec<u32> },
    Existence,
}

#[derive(Type, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Query {
    SelectWhere {
        table_name: ir::Path,
        select: SelectClause,
        r#where: WhereClause,
    },
}

#[derive(Type, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum QueryResponse {
    All { tuples: Vec<Vec<WireValue>> },
    One { values: Vec<WireValue> },
    Present,
    Absent,
}

#[derive(Type, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Mutation {
    Insert {
        table_name: ir::Path,
        columns: Vec<WireValue>,
    },
}

#[derive(Type, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MutationResponse {
    Id(TxnWireRowId),
}
