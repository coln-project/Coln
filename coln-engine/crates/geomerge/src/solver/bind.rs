use tracing::debug;

use crate::{
    ir,
    solver::{
        compile::{CompAtom, CompEq, CompLaw, CompProp, CompTerm, CompVal},
        matcher::term_matches,
    },
    store::Store,
    table::{CellValue, RowId, Table},
};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum BoundValue {
    RId(RowId),
    Cell(CellValue),
}

pub type Binding = Vec<Option<BoundValue>>;

fn bind_slot(binding: &mut Binding, slot: usize, value: BoundValue) -> bool {
    match binding.get_mut(slot) {
        Some(existing @ None) => {
            *existing = Some(value);
            true
        }
        Some(Some(v)) => *v == value,
        None => false,
    }
}

/// Bind a [`CompTerm`] to the `value`. If the term is not a Var, then check whether
/// it matches the value, otherwise check if the Var is already bound, and if so
/// whether the existing binding is valid
fn bind_term(binding: &mut Binding, term: &CompTerm, value: BoundValue) -> bool {
    match term {
        CompTerm::Var(slot) => bind_slot(binding, *slot, value),
        _ => term_matches(binding, term, &value),
    }
}

fn match_atom_row(
    table: &Table,
    row_idx: usize,
    atom: &CompAtom,
    binding: &Binding,
) -> Option<Binding> {
    if atom.table != *table.path() {
        return None;
    }
    let mut b = binding.clone();
    if let Some(term) = &atom.row_id {
        let rid = table.row_id_at(row_idx)?;
        let value = BoundValue::RId(rid);
        bind_term(&mut b, term, value).then_some(())?;
    }

    for CompVal { column_idx, term } in &atom.values {
        let value = table.cell_at(row_idx, *column_idx)?;
        bind_term(&mut b, term, BoundValue::Cell(value.clone())).then_some({})?
    }
    Some(b)
}

/// Resolve a `CompTerm` to a concrete `BoundValue` under the current binding.
///
/// Returns `None` iff the term is a `Var` whose slot is not yet bound.
pub fn eval_term(binding: &Binding, term: &CompTerm) -> Option<BoundValue> {
    match term {
        CompTerm::Var(slot) => binding.get(*slot).and_then(|v| v.clone()),
        CompTerm::Lit(ir::Lit::Int { value }) => Some(BoundValue::Cell(CellValue::Int(*value))),
        CompTerm::Lit(ir::Lit::String { value }) => {
            Some(BoundValue::Cell(CellValue::Str(value.clone())))
        }
    }
}

/// Antecedent equality evaluated as a pure filter: keep the binding iff both
/// sides are already bound and compare equal. Any unbound side is a compile
/// error the solver doesn't catch yet, so we panic on it.
fn apply_eq(binding: &Binding, eq: &CompEq) -> bool {
    match (eval_term(binding, &eq.left), eval_term(binding, &eq.right)) {
        (Some(l), Some(r)) => l == r,
        _ => unreachable!(
            "antecedent Eq {:?} had an unbound side under binding {:?}; \
             both sides must be bound by preceding atoms",
            eq, binding
        ),
    }
}

/// Filter `bindings` by the antecedent equality `eq`.
fn bind_eq(bindings: Vec<Binding>, eq: &CompEq) -> Vec<Binding> {
    bindings.into_iter().filter(|b| apply_eq(b, eq)).collect()
}

/// Extend `bindings` by joining with every row of `atom.table` that can match
/// `atom` under each current binding. Returns the (possibly empty) new set.
fn bind_atom(store: &Store, bindings: Vec<Binding>, atom: &CompAtom) -> Vec<Binding> {
    let Some(tbl) = store.table_at(&atom.table) else {
        return vec![];
    };
    let mut next = Vec::new();
    for binding in &bindings {
        for row_idx in 0..tbl.row_count() {
            if let Some(bound) = match_atom_row(tbl, row_idx, atom, binding) {
                next.push(bound);
            }
        }
    }
    next
}

/// Walk an antecedent `CompProp`, threading the binding set through each
/// conjunct. `Eq` is treated as a pure filter: both sides must already be
/// bound by preceding atoms (see `apply_eq`).
fn bind_prop(store: &Store, bindings: Vec<Binding>, prop: &CompProp) -> Vec<Binding> {
    match prop {
        CompProp::Atom(atom) => bind_atom(store, bindings, atom),
        CompProp::Eq(eq) => bind_eq(bindings, eq),
        CompProp::And(children) => {
            let mut current = bindings;
            for child in children {
                if current.is_empty() {
                    return current;
                }
                current = bind_prop(store, current, child);
            }
            current
        }
    }
}

pub fn bind_law(store: &Store, law: &CompLaw) -> Vec<Binding> {
    debug!(law_name = %law.path, law=?law, "binding vars for law");
    let initial = vec![vec![None; law.vars.len()]];
    bind_prop(store, initial, &law.antecedent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ir::{self, ColType, Path, PrimType, Schema},
        solver::compile::compile_law,
        table::CellValue,
    };

    #[test]
    fn bind_law_joins_antecedent_atoms() {
        let path = Path::from("T");
        let mut store = Store::new();
        store.insert_table(
            path.clone(),
            Table::new(
                path.clone(),
                Schema {
                    columns: vec![
                        ColType::PrimType {
                            prim: PrimType::PrimInt,
                        },
                        ColType::PrimType {
                            prim: PrimType::PrimInt,
                        },
                    ],
                    primary_key: None,
                },
            ),
        );

        let table = store.table_at_mut(&path).expect("table T");
        let op0 = table.add(vec![CellValue::Int(1), CellValue::Int(2)]);
        let op1 = table.add(vec![CellValue::Int(2), CellValue::Int(3)]);
        let op2 = table.add(vec![CellValue::Int(9), CellValue::Int(4)]);
        store.apply_batch(vec![op0, op1, op2]).expect("insert rows");

        let law = ir::LawEntry {
            path: Path::from("T.chain"),
            law: ir::Law {
                variables: vec![
                    ColType::PrimType {
                        prim: PrimType::PrimInt,
                    },
                    ColType::PrimType {
                        prim: PrimType::PrimInt,
                    },
                    ColType::PrimType {
                        prim: PrimType::PrimInt,
                    },
                ],
                antecedent: ir::Prop::And {
                    props: vec![
                        ir::Prop::Atom {
                            atom: ir::Atom {
                                table: path.clone(),
                                row_id: None,
                                values: vec![
                                    ir::ValueEntry {
                                        column: 0,
                                        term: ir::Term::Var { index: 0 },
                                    },
                                    ir::ValueEntry {
                                        column: 1,
                                        term: ir::Term::Var { index: 1 },
                                    },
                                ],
                            },
                        },
                        ir::Prop::Atom {
                            atom: ir::Atom {
                                table: path.clone(),
                                row_id: None,
                                values: vec![
                                    ir::ValueEntry {
                                        column: 0,
                                        term: ir::Term::Var { index: 1 },
                                    },
                                    ir::ValueEntry {
                                        column: 1,
                                        term: ir::Term::Var { index: 2 },
                                    },
                                ],
                            },
                        },
                    ],
                },
                consequent: ir::Prop::Atom {
                    atom: ir::Atom {
                        table: path.clone(),
                        row_id: None,
                        values: vec![ir::ValueEntry {
                            column: 0,
                            term: ir::Term::Var { index: 0 },
                        }],
                    },
                },
            },
        };

        let compiled = compile_law(&law).expect("compile law");
        let bindings = bind_law(&store, &compiled);

        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0][0], Some(BoundValue::Cell(CellValue::Int(1))));
        assert_eq!(bindings[0][1], Some(BoundValue::Cell(CellValue::Int(2))));
        assert_eq!(bindings[0][2], Some(BoundValue::Cell(CellValue::Int(3))));
    }

    /// Build a single-column `Int` table `T` populated with the supplied
    /// values, returning the store.
    fn store_with_int_column(values: &[i64]) -> (Store, Path) {
        let path = Path::from("T");
        let mut store = Store::new();
        store.insert_table(
            path.clone(),
            Table::new(
                path.clone(),
                Schema {
                    columns: vec![ColType::PrimType {
                        prim: PrimType::PrimInt,
                    }],
                    primary_key: None,
                },
            ),
        );
        let table = store.table_at_mut(&path).expect("table T");
        let ops = values
            .iter()
            .map(|v| table.add(vec![CellValue::Int(*v)]))
            .collect::<Vec<_>>();
        store.apply_batch(ops).expect("insert rows");
        (store, path)
    }

    #[test]
    fn eq_in_antecedent_filters_to_matching_rows() {
        let (store, path) = store_with_int_column(&[1, 2, 3]);

        let law = ir::LawEntry {
            path: Path::from("T.eq_filter"),
            law: ir::Law {
                variables: vec![
                    ColType::PrimType {
                        prim: PrimType::PrimInt,
                    },
                    ColType::PrimType {
                        prim: PrimType::PrimInt,
                    },
                ],
                antecedent: ir::Prop::And {
                    props: vec![
                        ir::Prop::Atom {
                            atom: ir::Atom {
                                table: path.clone(),
                                row_id: None,
                                values: vec![ir::ValueEntry {
                                    column: 0,
                                    term: ir::Term::Var { index: 0 },
                                }],
                            },
                        },
                        ir::Prop::Atom {
                            atom: ir::Atom {
                                table: path.clone(),
                                row_id: None,
                                values: vec![ir::ValueEntry {
                                    column: 0,
                                    term: ir::Term::Var { index: 1 },
                                }],
                            },
                        },
                        ir::Prop::Eq {
                            left: ir::Term::Var { index: 0 },
                            right: ir::Term::Var { index: 1 },
                        },
                    ],
                },
                consequent: ir::Prop::Atom {
                    atom: ir::Atom {
                        table: path.clone(),
                        row_id: None,
                        values: vec![ir::ValueEntry {
                            column: 0,
                            term: ir::Term::Var { index: 0 },
                        }],
                    },
                },
            },
        };

        let compiled = compile_law(&law).expect("compile law");
        let bindings = bind_law(&store, &compiled);

        assert_eq!(bindings.len(), 3);
        for b in &bindings {
            assert_eq!(b[0], b[1]);
        }
    }

    #[test]
    fn eq_with_literal_in_antecedent_pins_var_to_value() {
        let (store, path) = store_with_int_column(&[1, 2, 3]);

        let law = ir::LawEntry {
            path: Path::from("T.eq_literal"),
            law: ir::Law {
                variables: vec![ColType::PrimType {
                    prim: PrimType::PrimInt,
                }],
                antecedent: ir::Prop::And {
                    props: vec![
                        ir::Prop::Atom {
                            atom: ir::Atom {
                                table: path.clone(),
                                row_id: None,
                                values: vec![ir::ValueEntry {
                                    column: 0,
                                    term: ir::Term::Var { index: 0 },
                                }],
                            },
                        },
                        ir::Prop::Eq {
                            left: ir::Term::Var { index: 0 },
                            right: ir::Term::Lit {
                                lit: ir::Lit::Int { value: 2 },
                            },
                        },
                    ],
                },
                consequent: ir::Prop::Atom {
                    atom: ir::Atom {
                        table: path.clone(),
                        row_id: None,
                        values: vec![ir::ValueEntry {
                            column: 0,
                            term: ir::Term::Var { index: 0 },
                        }],
                    },
                },
            },
        };

        let compiled = compile_law(&law).expect("compile law");
        let bindings = bind_law(&store, &compiled);

        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0][0], Some(BoundValue::Cell(CellValue::Int(2))));
    }
}
