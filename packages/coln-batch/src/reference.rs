// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Brute-force reference executor — the test oracle.
//!
//! Evaluates a query by trying every combination of rows, atom by atom.
//! Exponential in the number of atoms; intended only for small inputs,
//! where it establishes ground truth for the real executors. The
//! implementation is deliberately minimal so that its correctness can be
//! verified by inspection. It compares keys like the real executors;
//! key equality is value equality within a column type.

use anyhow::Result;

use crate::query::{KeyAtom, KeyTerm, Query, Tables, prepare};
use crate::relation::Relation;
use crate::table::SortedTable;
use crate::types::Key;

/// Evaluate `query` against `tables` by exhaustive search. Returns the
/// projected result, sorted and deduplicated (set semantics).
///
/// It reads each relation by a plain scan in schema order and uses none of
/// the searches, so it also serves to cross-check a storage layer's tables.
pub fn execute(query: &Query, tables: &dyn Tables) -> Result<Relation> {
    let prepared = prepare(query, tables)?;
    let Some(atoms) = prepared.atoms else {
        return Ok(Relation::empty("result", prepared.schema));
    };
    let scanned: Vec<Box<dyn SortedTable + '_>> = atoms
        .iter()
        .map(|a| {
            let identity: Vec<usize> = (0..tables.schema(&a.relation)?.arity()).collect();
            tables.sorted(&a.relation, &identity)
        })
        .collect::<Result<_>>()?;
    let tables: Vec<&dyn SortedTable> = scanned.iter().map(|t| &**t).collect();

    let mut binding: Vec<Option<Key>> = vec![None; query.num_vars()];
    let mut out: Vec<Key> = Vec::new();
    search(query, &atoms, &tables, 0, &mut binding, &mut out);

    Ok(Relation::from_flat_rows("result", prepared.schema, &out).distinct())
}

fn search(
    query: &Query,
    atoms: &[KeyAtom],
    tables: &[&dyn SortedTable],
    atom_idx: usize,
    binding: &mut Vec<Option<Key>>,
    out: &mut Vec<Key>,
) {
    if atom_idx == atoms.len() {
        for &v in &query.head {
            out.push(binding[v].expect("head variable bound (validated)"));
        }
        return;
    }
    let atom = &atoms[atom_idx];
    let table = tables[atom_idx];
    'rows: for r in 0..table.len() {
        // Try to unify this row with the atom's terms.
        let mut newly_bound: Vec<usize> = Vec::new();
        for (c, term) in atom.terms.iter().enumerate() {
            let value = table.value(r, c);
            match term {
                KeyTerm::Lit(l) => {
                    if value != *l {
                        undo(binding, &newly_bound);
                        continue 'rows;
                    }
                }
                KeyTerm::Var(v) => match binding[*v] {
                    Some(bound) => {
                        if value != bound {
                            undo(binding, &newly_bound);
                            continue 'rows;
                        }
                    }
                    None => {
                        binding[*v] = Some(value);
                        newly_bound.push(*v);
                    }
                },
            }
        }
        search(query, atoms, tables, atom_idx + 1, binding, out);
        undo(binding, &newly_bound);
    }
}

fn undo(binding: &mut [Option<Key>], newly_bound: &[usize]) {
    for &v in newly_bound {
        binding[v] = None;
    }
}
