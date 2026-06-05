use tracing::debug;

use crate::{
    ir,
    solver::{
        bind::{Binding, BoundValue},
        compile::CompTerm,
    },
    table::CellValue,
};

/// Check if an _already_ bound variable slot matches value.
pub(crate) fn boundvar_matches(binding: &Binding, slot: usize, value: &BoundValue) -> bool {
    match binding.get(slot) {
        Some(Some(bound)) => match (bound, value) {
            (BoundValue::RId(a), BoundValue::Cell(CellValue::Id(b)))
            | (BoundValue::Cell(CellValue::Id(a)), BoundValue::RId(b)) => {
                debug!(bound=?bound, value = ?value, "matching");
                a == b
            }
            _ => bound == value,
        },

        None => unreachable!("Cannot be called with unbound value"),
        _ => false,
    }
}

/// Match a term against a value. This is only called for terms that are not Var
/// variant. The Var variant has its own checking logic in bind.rs
pub(crate) fn term_matches(binding: &Binding, term: &CompTerm, value: &BoundValue) -> bool {
    match term {
        CompTerm::Var(slot) => boundvar_matches(binding, *slot, value),
        CompTerm::Lit(ir::Lit::Int { value: expected }) => {
            *value == BoundValue::Cell(CellValue::Int(*expected))
        }
        CompTerm::Lit(ir::Lit::String { value: expected }) => {
            *value == BoundValue::Cell(CellValue::Str(expected.clone()))
        }
    }
}
