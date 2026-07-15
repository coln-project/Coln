// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Executor 1: a left-deep chain of binary hash joins.
//!
//! Atoms are processed in query order. The intermediate result is a flat
//! table of bindings for all variables seen so far; each step hash-joins
//! it with the next atom on their shared variables. This is the classic
//! plan shape — solid on acyclic queries, and the executor whose oversized
//! intermediates on *cyclic* queries motivate the generic join
//! ([`crate::generic_join`]).
//!
//! Data is read exclusively through the [`SortedTable`] trait (a plain
//! scan here — hash joins need no ordering), so any storage back end works.

use std::collections::HashMap;

use anyhow::Result;

use crate::query::{Catalog, Query, Term, VarId};
use crate::relation::Relation;
use crate::table::{ArrowSortedTable, SortedTable};

/// Evaluate `query` against `catalog` with a chain of hash joins. Returns
/// the projected result, sorted and deduplicated (set semantics).
pub fn execute(query: &Query, catalog: &Catalog) -> Result<Relation> {
    catalog.check(query)?;
    let empty = |query: &Query| {
        Relation::from_flat_rows("result", query.head_names(), query.head.len(), &[])
    };

    // Intermediate result: `n_rows` rows of `width` values; `bound[v]`
    // gives the column of variable v. Starts as a single zero-width row.
    let mut bound: Vec<Option<usize>> = vec![None; query.num_vars()];
    let mut width = 0usize;
    let mut n_rows = 1usize;
    let mut data: Vec<u64> = Vec::new();

    for atom in &query.atoms {
        let rel = catalog.get(&atom.relation)?;
        let identity: Vec<usize> = (0..rel.arity()).collect();
        let table = ArrowSortedTable::from_relation(rel, identity)?;

        // Classify the atom's columns.
        let mut lit_checks: Vec<(usize, u64)> = Vec::new(); // (atom col, value)
        let mut key_pairs: Vec<(usize, usize)> = Vec::new(); // (interm. col, atom col)
        let mut new_vars: Vec<(VarId, usize)> = Vec::new(); // (var, atom col)
        let mut intra_eq: Vec<(usize, usize)> = Vec::new(); // repeated var in atom
        let mut first_col: HashMap<VarId, usize> = HashMap::new();
        for (c, term) in atom.terms.iter().enumerate() {
            match term {
                Term::Lit(x) => lit_checks.push((c, *x)),
                Term::Var(v) => {
                    if let Some(&c0) = first_col.get(v) {
                        intra_eq.push((c0, c));
                    } else {
                        first_col.insert(*v, c);
                        match bound[*v] {
                            Some(icol) => key_pairs.push((icol, c)),
                            None => new_vars.push((*v, c)),
                        }
                    }
                }
            }
        }

        // Build the hash index over the atom's rows (post-filter).
        let mut index: HashMap<Vec<u64>, Vec<usize>> = HashMap::new();
        for r in 0..table.len() {
            if lit_checks.iter().any(|&(c, x)| table.value(r, c) != x) {
                continue;
            }
            if intra_eq
                .iter()
                .any(|&(a, b)| table.value(r, a) != table.value(r, b))
            {
                continue;
            }
            let key: Vec<u64> = key_pairs.iter().map(|&(_, c)| table.value(r, c)).collect();
            index.entry(key).or_default().push(r);
        }

        // An atom that binds nothing new and shares nothing acts as a
        // pure existence filter.
        if key_pairs.is_empty() && new_vars.is_empty() {
            if index.is_empty() {
                return Ok(empty(query));
            }
            continue;
        }

        // Probe.
        let new_width = width + new_vars.len();
        let mut out: Vec<u64> = Vec::new();
        for i in 0..n_rows {
            let row = &data[i * width..(i + 1) * width];
            let key: Vec<u64> = key_pairs.iter().map(|&(icol, _)| row[icol]).collect();
            if let Some(matches) = index.get(&key) {
                for &r in matches {
                    out.extend_from_slice(row);
                    for &(_, c) in &new_vars {
                        out.push(table.value(r, c));
                    }
                }
            }
        }

        for (k, &(v, _)) in new_vars.iter().enumerate() {
            bound[v] = Some(width + k);
        }
        width = new_width;
        data = out;
        n_rows = data.len() / width;
        if n_rows == 0 {
            return Ok(empty(query));
        }
    }

    // Project the head.
    let head_cols: Vec<usize> = query
        .head
        .iter()
        .map(|&v| bound[v].expect("head variable bound (validated)"))
        .collect();
    let mut out: Vec<u64> = Vec::with_capacity(n_rows * head_cols.len());
    for i in 0..n_rows {
        let row = &data[i * width..(i + 1) * width];
        for &c in &head_cols {
            out.push(row[c]);
        }
    }
    Ok(
        Relation::from_flat_rows("result", query.head_names(), query.head.len(), &out)
            .sorted_dedup(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::Atom;

    #[test]
    fn two_atom_chain() {
        let mut cat = Catalog::new();
        cat.insert(Relation::new(
            "R",
            ["a", "b"],
            vec![vec![1, 2, 1], vec![2, 3, 4]],
        ));
        cat.insert(Relation::new(
            "S",
            ["a", "b"],
            vec![vec![2, 4, 9], vec![5, 7, 9]],
        ));
        // Q(x,y,z) ← R(x,y), S(y,z)
        let q = Query {
            var_names: vec!["x".into(), "y".into(), "z".into()],
            atoms: vec![
                Atom {
                    relation: "R".into(),
                    terms: vec![Term::Var(0), Term::Var(1)],
                },
                Atom {
                    relation: "S".into(),
                    terms: vec![Term::Var(1), Term::Var(2)],
                },
            ],
            head: vec![0, 1, 2],
        };
        let result = execute(&q, &cat).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result.row(0), vec![1, 2, 5]);
        assert_eq!(result.row(1), vec![1, 4, 7]);
    }
}
