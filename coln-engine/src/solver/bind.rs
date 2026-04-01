use crate::{
    solver::{
        compile::{CompAtom, CompLaw, CompTerm, CompVal},
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
        bind_term(&mut b, term, BoundValue::Cell(value.clone())).then(|| {})?
    }
    Some(b)
}

pub fn bind_law(store: &Store, law: &CompLaw) -> Vec<Binding> {
    let mut bindings = vec![vec![None; law.vars.len()]];

    for atom in &law.antecedent {
        let Some(tbl) = store.table_at(&atom.table) else {
            return vec![];
        };

        let mut next_bindings = vec![];
        for binding in &bindings {
            for row_idx in 0..tbl.row_count() {
                if let Some(bound) = match_atom_row(tbl, row_idx, atom, binding) {
                    next_bindings.push(bound);
                }
            }
        }

        if next_bindings.is_empty() {
            return vec![];
        }

        bindings = next_bindings;
    }

    bindings
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

        let table = store.table_at(&path).expect("table T");
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
}
