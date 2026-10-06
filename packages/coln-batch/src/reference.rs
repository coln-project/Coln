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
//! key equality is value equality within a column type. A combination of
//! rows weighs the product of their weights.

use anyhow::Result;

use crate::query::{Catalog, KeyAtom, KeyTerm, Query};
use crate::relation::Relation;
use crate::types::{Key, Weight, mul_weights};

/// Evaluate `query` against `catalog` by exhaustive search. Returns the
/// projected result as a Z-set in normal form: a head row weighs the sum,
/// over the combinations of rows that produce it, of their weights'
/// product.
pub fn execute(query: &Query, catalog: &Catalog) -> Result<Relation> {
    let prepared = catalog.prepare(query)?;
    let Some(atoms) = prepared.atoms else {
        return Ok(Relation::empty("result", prepared.schema));
    };
    let tables: Vec<&Relation> = atoms
        .iter()
        .map(|a| catalog.get(&a.relation))
        .collect::<Result<_>>()?;

    let mut binding: Vec<Option<Key>> = vec![None; query.num_vars()];
    let mut out = Output::default();
    search(query, &atoms, &tables, 0, 1, &mut binding, &mut out);

    Ok(Relation::from_flat_rows("result", prepared.schema, &out.keys, out.weights).consolidate())
}

/// The head rows found so far, flat, with one weight per row.
#[derive(Default)]
struct Output {
    keys: Vec<Key>,
    weights: Vec<Weight>,
}

/// Extend the combination of rows chosen for the atoms before `atom_idx`,
/// which weighs `weight`, by every row of the next atom that fits.
fn search(
    query: &Query,
    atoms: &[KeyAtom],
    tables: &[&Relation],
    atom_idx: usize,
    weight: Weight,
    binding: &mut Vec<Option<Key>>,
    out: &mut Output,
) {
    if atom_idx == atoms.len() {
        for &v in &query.head {
            out.keys
                .push(binding[v].expect("head variable bound (validated)"));
        }
        out.weights.push(weight);
        return;
    }
    let atom = &atoms[atom_idx];
    let table = tables[atom_idx];
    'rows: for r in 0..table.len() {
        // Try to unify this row with the atom's terms.
        let mut newly_bound: Vec<usize> = Vec::new();
        for (c, term) in atom.terms.iter().enumerate() {
            let value = table.cols[c][r];
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
        let weight = mul_weights(weight, table.weight(r));
        search(query, atoms, tables, atom_idx + 1, weight, binding, out);
        undo(binding, &newly_bound);
    }
}

fn undo(binding: &mut [Option<Key>], newly_bound: &[usize]) {
    for &v in newly_bound {
        binding[v] = None;
    }
}
