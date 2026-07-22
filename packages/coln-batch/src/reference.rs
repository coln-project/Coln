// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Brute-force reference executor — the test oracle.
//!
//! Evaluates a query by trying every combination of rows, atom by atom.
//! Exponential in the number of atoms; intended only for small inputs,
//! where it establishes ground truth for the real executors. The
//! implementation is deliberately minimal so that its correctness can be
//! verified by inspection.

use anyhow::Result;

use crate::query::{Catalog, Query, Term};
use crate::relation::Relation;

/// Evaluate `query` against `catalog` by exhaustive search. Returns the
/// projected result, sorted and deduplicated (set semantics).
pub fn execute(query: &Query, catalog: &Catalog) -> Result<Relation> {
    catalog.check(query)?;
    let tables: Vec<&Relation> = query
        .atoms
        .iter()
        .map(|a| catalog.get(&a.relation))
        .collect::<Result<_>>()?;

    let mut binding: Vec<Option<u64>> = vec![None; query.num_vars()];
    let mut out: Vec<u64> = Vec::new();
    search(query, &tables, 0, &mut binding, &mut out);

    Ok(
        Relation::from_flat_rows("result", query.head_names(), query.head.len(), &out)
            .sorted_dedup(),
    )
}

fn search(
    query: &Query,
    tables: &[&Relation],
    atom_idx: usize,
    binding: &mut Vec<Option<u64>>,
    out: &mut Vec<u64>,
) {
    if atom_idx == query.atoms.len() {
        for &v in &query.head {
            out.push(binding[v].expect("head variable bound (validated)"));
        }
        return;
    }
    let atom = &query.atoms[atom_idx];
    let table = tables[atom_idx];
    'rows: for r in 0..table.len() {
        // Try to unify this row with the atom's terms.
        let mut newly_bound: Vec<usize> = Vec::new();
        for (c, term) in atom.terms.iter().enumerate() {
            let value = table.cols[c][r];
            match term {
                Term::Lit(l) => {
                    if value != *l {
                        undo(binding, &newly_bound);
                        continue 'rows;
                    }
                }
                Term::Var(v) => match binding[*v] {
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
        search(query, tables, atom_idx + 1, binding, out);
        undo(binding, &newly_bound);
    }
}

fn undo(binding: &mut [Option<u64>], newly_bound: &[usize]) {
    for &v in newly_bound {
        binding[v] = None;
    }
}
