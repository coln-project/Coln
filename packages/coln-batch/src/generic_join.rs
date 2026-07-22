// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Executor 2: the worst-case-optimal **generic join**.
//!
//! Instead of joining two relations at a time, the generic join solves the
//! query one *variable* at a time. For each variable (in `VarId` order) it
//! intersects the candidate values of every atom containing that variable
//! by leapfrogging: repeated seeks (`lower_bound`) into each atom's sorted
//! index, always jumping to the largest candidate seen so far. Matching
//! values narrow each atom to the row range agreeing with the binding so
//! far (`equal_range`), and the search descends to the next variable.
//! No intermediate result is ever materialized, which is what makes the
//! algorithm worst-case optimal on cyclic queries.
//!
//! Requirements on storage are exactly the [`SortedTable`] trait: one
//! index per atom, sorted by the atom's literal columns first, then its
//! variable columns in elimination order. [`execute`] builds those indexes
//! in memory ([`ArrowSortedTable`]); a storage layer can serve them
//! instead without touching this module's search logic.

use std::ops::Range;

use anyhow::Result;

use crate::query::{Catalog, Query, Term, VarId};
use crate::relation::Relation;
use crate::table::{ArrowSortedTable, SortedTable};

/// Evaluate `query` against `catalog` with the generic join. Returns the
/// projected result, sorted and deduplicated (set semantics).
pub fn execute(query: &Query, catalog: &Catalog) -> Result<Relation> {
    catalog.check(query)?;

    // Build one suitably-sorted index per atom.
    // TODO(perf): sorting happens here, per query. Once the storage layer
    // serves SortedTable directly (pre-built indexes), this block becomes
    // a lookup instead of an O(N log N) build.
    let mut atoms: Vec<AtomExec<ArrowSortedTable>> = Vec::with_capacity(query.atoms.len());
    for atom in &query.atoms {
        let rel = catalog.get(&atom.relation)?;

        // Column order: literal columns first, then variable columns in
        // elimination (VarId) order; a variable's columns end up adjacent.
        let mut order: Vec<usize> = (0..atom.terms.len()).collect();
        order.sort_by_key(|&c| match atom.terms[c] {
            Term::Lit(_) => (0, 0, c),
            Term::Var(v) => (1, v, c),
        });

        let mut lit_prefix: Vec<u64> = Vec::new();
        let mut var_depths: Vec<Vec<usize>> = vec![Vec::new(); query.num_vars()];
        for (depth, &c) in order.iter().enumerate() {
            match atom.terms[c] {
                Term::Lit(x) => lit_prefix.push(x),
                Term::Var(v) => var_depths[v].push(depth),
            }
        }

        let table = ArrowSortedTable::from_relation(rel, order)?;
        atoms.push(AtomExec {
            table,
            lit_prefix,
            var_depths,
        });
    }

    // Narrow every atom by its literal prefix; an empty range anywhere
    // means the whole result is empty.
    let mut ranges: Vec<Range<usize>> = Vec::with_capacity(atoms.len());
    for a in &atoms {
        let mut r = 0..a.table.len();
        for (depth, &x) in a.lit_prefix.iter().enumerate() {
            r = a.table.equal_range(depth, x, r.start, r.end);
        }
        if r.is_empty() {
            return Ok(Relation::from_flat_rows(
                "result",
                query.head_names(),
                query.head.len(),
                &[],
            ));
        }
        ranges.push(r);
    }

    // Which atoms participate in each variable.
    let participants: Vec<Vec<usize>> = (0..query.num_vars())
        .map(|v| {
            (0..atoms.len())
                .filter(|&i| !atoms[i].var_depths[v].is_empty())
                .collect()
        })
        .collect();

    let solver = Solver {
        atoms: &atoms,
        participants: &participants,
        query,
    };
    let mut binding = vec![0u64; query.num_vars()];
    let mut out: Vec<u64> = Vec::new();
    solver.solve(0, &mut ranges, &mut binding, &mut out);

    Ok(
        Relation::from_flat_rows("result", query.head_names(), query.head.len(), &out)
            .sorted_dedup(),
    )
}

/// One atom, ready for execution: its sorted index plus the mapping from
/// query variables to the index's sort depths.
struct AtomExec<T: SortedTable> {
    table: T,
    /// Values of the literal columns, by depth `0..lit_prefix.len()`.
    lit_prefix: Vec<u64>,
    /// For each variable: the sort depths of its columns in this atom
    /// (adjacent by construction; empty if the variable does not occur).
    var_depths: Vec<Vec<usize>>,
}

struct Solver<'a, T: SortedTable> {
    atoms: &'a [AtomExec<T>],
    participants: &'a [Vec<usize>],
    query: &'a Query,
}

impl<T: SortedTable> Solver<'_, T> {
    /// Bind variable `v` to every value in the intersection of the
    /// participating atoms' candidate sets, recursing per value.
    fn solve(
        &self,
        v: VarId,
        ranges: &mut [Range<usize>],
        binding: &mut [u64],
        out: &mut Vec<u64>,
    ) {
        if v == self.query.num_vars() {
            for &h in &self.query.head {
                out.push(binding[h]);
            }
            return;
        }
        let parts = &self.participants[v];
        debug_assert!(!parts.is_empty(), "validated: every var occurs somewhere");

        let mut cand: u64 = 0;
        loop {
            // Leapfrog: advance `cand` until every participating atom
            // contains it.
            let mut agreed = true;
            for &p in parts {
                let a = &self.atoms[p];
                let d = a.var_depths[v][0];
                let r = &ranges[p];
                let pos = a.table.lower_bound(d, cand, r.start, r.end);
                if pos == r.end {
                    return; // one atom is exhausted — no further values
                }
                let w = a.table.value(pos, a.table.sort_order()[d]);
                if w > cand {
                    cand = w;
                    agreed = false;
                }
            }
            if agreed {
                // Narrow every participant through all its columns of v.
                // (A repeated variable inside one atom has several depths;
                // narrowing can then come up empty — that value is skipped.)
                let saved: Vec<(usize, Range<usize>)> =
                    parts.iter().map(|&p| (p, ranges[p].clone())).collect();
                let mut ok = true;
                for &p in parts {
                    let a = &self.atoms[p];
                    let mut r = ranges[p].clone();
                    for &d in &a.var_depths[v] {
                        r = a.table.equal_range(d, cand, r.start, r.end);
                        if r.is_empty() {
                            ok = false;
                            break;
                        }
                    }
                    ranges[p] = r;
                    if !ok {
                        break;
                    }
                }
                if ok {
                    binding[v] = cand;
                    self.solve(v + 1, ranges, binding, out);
                }
                for (p, r) in saved {
                    ranges[p] = r;
                }
                if cand == u64::MAX {
                    return;
                }
                cand += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::Atom;

    #[test]
    fn triangle_on_hand_built_graph() {
        // Edges: 1→2, 2→3, 3→1 (a triangle) plus noise 1→3.
        let mut cat = Catalog::new();
        cat.insert(Relation::new(
            "R_f",
            ["src", "dst"],
            vec![vec![1, 1], vec![2, 3]],
        ));
        cat.insert(Relation::new("R_g", ["src", "dst"], vec![vec![2], vec![3]]));
        cat.insert(Relation::new("R_h", ["src", "dst"], vec![vec![3], vec![1]]));
        let q = crate::fixtures::triangle_query();
        let result = execute(&q, &cat).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result.row(0), vec![1, 2, 3]);
    }

    #[test]
    fn repeated_variable_within_one_atom() {
        // Q(x) ← T(x, x)
        let mut cat = Catalog::new();
        cat.insert(Relation::new(
            "T",
            ["a", "b"],
            vec![vec![4, 5, 7], vec![4, 6, 7]],
        ));
        let q = Query {
            var_names: vec!["x".into()],
            atoms: vec![Atom {
                relation: "T".into(),
                terms: vec![Term::Var(0), Term::Var(0)],
            }],
            head: vec![0],
        };
        let result = execute(&q, &cat).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result.row(0), vec![4]);
        assert_eq!(result.row(1), vec![7]);
    }
}
