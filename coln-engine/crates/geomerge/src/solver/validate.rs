use std::error::Error;
use std::fmt;
use tracing::debug;

use crate::{
    solver::{
        bind::{Binding, BoundValue, bind_law, eval_term},
        compile::{CompAtom, CompEq, CompLaw, CompProp, CompVal},
        matcher::term_matches,
    },
    store::Store,
    table::CellValue,
};

/// Why a law was violated at a given binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViolationCause {
    /// A required consequent atom was not present in the store.
    MissingAtom(CompAtom),
    /// An equality in the consequent did not hold.
    UnsatisfiedEq(CompEq),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawViolation {
    pub law: CompLaw,
    pub cause: ViolationCause,
    pub binding: Binding,
}

impl fmt::Display for LawViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.cause {
            ViolationCause::MissingAtom(atom) => write!(
                f,
                "law {} violated: missing consequent atom in table {}",
                self.law, atom.table
            ),
            ViolationCause::UnsatisfiedEq(eq) => write!(
                f,
                "law {} violated: equality {:?} = {:?} did not hold",
                self.law, eq.left, eq.right
            ),
        }
    }
}

impl Error for LawViolation {}

// TODO: completely ignoring efficiency for now
pub fn consequent_atom_holds(store: &Store, atom: &CompAtom, binding: &Binding) -> bool {
    let Some(table) = store.table_at(&atom.table) else {
        debug!(table = ?atom.table, "consequent table missing from store");
        return false;
    };
    debug!(atom = ?atom, binding=?binding, "checking");
    // Check each row in the table, if any one row satisfies, then return true
    'row: for row_idx in 0..table.row_count() {
        if let Some(term) = &atom.row_id {
            let Some(rid) = table.row_id_at(row_idx) else {
                continue;
            };
            let value = BoundValue::RId(rid);
            if !term_matches(binding, term, &value) {
                continue;
            }
        }

        for CompVal { column_idx, term } in &atom.values {
            let Some(value) = table.cell_at(row_idx, *column_idx) else {
                continue 'row;
            };
            if !term_matches(binding, term, &BoundValue::Cell(value.clone())) {
                continue 'row;
            }
        }
        return true;
    }

    false
}

/// Evaluate a consequent equality against the current binding.
pub fn consequent_eq_holds(binding: &Binding, eq: &CompEq) -> bool {
    let (Some(l), Some(r)) = (eval_term(binding, &eq.left), eval_term(binding, &eq.right)) else {
        debug!(eq = ?eq, binding = ?binding, "eq term not bound");
        return false;
    };
    match (&l, &r) {
        // Row ids and entity cells refer to the same identity when equal.
        (BoundValue::RId(a), BoundValue::Cell(CellValue::Id(b)))
        | (BoundValue::Cell(CellValue::Id(a)), BoundValue::RId(b)) => a == b,
        _ => l == r,
    }
}

/// Check whether a consequent proposition holds under `binding`.
///
/// On failure, returns the first leaf (atom or equality) that failed so the
/// caller can build a precise `LawViolation`.
fn prop_holds(store: &Store, binding: &Binding, prop: &CompProp) -> Result<(), ViolationCause> {
    match prop {
        CompProp::Atom(atom) => {
            if consequent_atom_holds(store, atom, binding) {
                Ok(())
            } else {
                Err(ViolationCause::MissingAtom(atom.clone()))
            }
        }
        CompProp::Eq(eq) => {
            if consequent_eq_holds(binding, eq) {
                Ok(())
            } else {
                Err(ViolationCause::UnsatisfiedEq(eq.clone()))
            }
        }
        CompProp::And(children) => {
            for child in children {
                prop_holds(store, binding, child)?;
            }
            Ok(())
        }
    }
}

pub fn check_law(store: &Store, law: &CompLaw) -> Result<(), Box<LawViolation>> {
    let bindings = bind_law(store, law);
    debug!(law = %law.path, bindings = ?bindings, "checking law with bindings");

    for binding in bindings {
        if let Err(cause) = prop_holds(store, &binding, &law.consequent) {
            debug!(law = %law.path, cause = ?cause, "law violation detected");
            return Err(Box::new(LawViolation {
                law: law.clone(),
                cause,
                binding,
            }));
        }
    }
    debug!(law = %law.path, "law satisfied");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ir::{self, ColType, Path, PrimType, Schema},
        solver::compile::compile_law,
        table::{CellValue, Table},
    };

    #[test]
    fn true_antecedent_still_checks_consequent() {
        let g0_path = Path::from("G0");
        let mut store = Store::new();
        store.insert_table(
            g0_path.clone(),
            Table::new(
                g0_path.clone(),
                Schema {
                    columns: vec![],
                    primary_key: None,
                },
            ),
        );

        let law = ir::LawEntry {
            path: Path::from("G0.total"),
            law: ir::Law {
                variables: vec![],
                antecedent: ir::Prop::And { props: vec![] },
                consequent: ir::Prop::Atom {
                    atom: ir::Atom {
                        table: g0_path.clone(),
                        row_id: None,
                        values: vec![],
                    },
                },
            },
        };

        let compiled = compile_law(&law).expect("compile law");
        let violation = check_law(&store, &compiled).expect_err("missing G0 row should violate");

        assert_eq!(violation.law.path, Path::from("G0.total"));
        match &violation.cause {
            ViolationCause::MissingAtom(atom) => assert_eq!(atom.table, g0_path),
            other => panic!("expected MissingAtom, got {:?}", other),
        }
        assert!(violation.binding.is_empty());
    }

    #[test]
    fn antecedent_binding_satisfies_consequent() {
        let source = Path::from("Src");
        let target = Path::from("Dst");
        let mut store = Store::new();
        store.insert_table(
            source.clone(),
            Table::new(
                source.clone(),
                Schema {
                    columns: vec![ColType::PrimType {
                        prim: PrimType::PrimInt,
                    }],
                    primary_key: None,
                },
            ),
        );
        store.insert_table(
            target.clone(),
            Table::new(
                target.clone(),
                Schema {
                    columns: vec![ColType::PrimType {
                        prim: PrimType::PrimInt,
                    }],
                    primary_key: None,
                },
            ),
        );

        let src = store.table_at_mut(&source).expect("Src table");
        let src_row = src.add(vec![CellValue::Int(7)]);
        let dst = store.table_at_mut(&target).expect("Dst table");
        let dst_row = dst.add(vec![CellValue::Int(7)]);
        store
            .apply_batch(vec![src_row, dst_row])
            .expect("insert matching rows");

        let law = ir::LawEntry {
            path: Path::from("Copy.total"),
            law: ir::Law {
                variables: vec![ColType::PrimType {
                    prim: PrimType::PrimInt,
                }],
                antecedent: ir::Prop::Atom {
                    atom: ir::Atom {
                        table: source,
                        row_id: None,
                        values: vec![ir::ValueEntry {
                            column: 0,
                            term: ir::Term::Var { index: 0 },
                        }],
                    },
                },
                consequent: ir::Prop::Atom {
                    atom: ir::Atom {
                        table: target,
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
        assert!(check_law(&store, &compiled).is_ok());
    }

    #[test]
    fn foreign_key_law_fails_when_only_referencing_row_exists() {
        let left = Path::from("Left");
        let right = Path::from("Right");
        let link = Path::from("Link");
        let mut store = Store::new();
        store.insert_table(
            left.clone(),
            Table::new(
                left.clone(),
                Schema {
                    columns: vec![ColType::PrimType {
                        prim: PrimType::PrimInt,
                    }],
                    primary_key: None,
                },
            ),
        );
        store.insert_table(
            right.clone(),
            Table::new(
                right.clone(),
                Schema {
                    columns: vec![ColType::PrimType {
                        prim: PrimType::PrimInt,
                    }],
                    primary_key: None,
                },
            ),
        );
        store.insert_table(
            link.clone(),
            Table::new(
                link.clone(),
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

        let law = ir::LawEntry {
            path: Path::from("Link.foreignKeys"),
            law: ir::Law {
                variables: vec![
                    ColType::PrimType {
                        prim: PrimType::PrimInt,
                    },
                    ColType::PrimType {
                        prim: PrimType::PrimInt,
                    },
                ],
                antecedent: ir::Prop::Atom {
                    atom: ir::Atom {
                        table: link.clone(),
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
                consequent: ir::Prop::And {
                    props: vec![
                        ir::Prop::Atom {
                            atom: ir::Atom {
                                table: left.clone(),
                                row_id: None,
                                values: vec![ir::ValueEntry {
                                    column: 0,
                                    term: ir::Term::Var { index: 0 },
                                }],
                            },
                        },
                        ir::Prop::Atom {
                            atom: ir::Atom {
                                table: right.clone(),
                                row_id: None,
                                values: vec![ir::ValueEntry {
                                    column: 0,
                                    term: ir::Term::Var { index: 1 },
                                }],
                            },
                        },
                    ],
                },
            },
        };

        let compiled = compile_law(&law).expect("compile law");

        let only_link = store
            .table_at_mut(&link)
            .expect("Link table")
            .add(vec![CellValue::Int(10), CellValue::Int(20)]);
        store
            .apply_batch(vec![only_link])
            .expect("insert referencing row");

        let violation = check_law(&store, &compiled).expect_err("missing referenced rows");
        assert_eq!(violation.law.path, Path::from("Link.foreignKeys"));
        match &violation.cause {
            ViolationCause::MissingAtom(atom) => assert_eq!(atom.table, left),
            other => panic!("expected MissingAtom, got {:?}", other),
        }
        assert_eq!(
            violation.binding,
            vec![
                Some(BoundValue::Cell(CellValue::Int(10))),
                Some(BoundValue::Cell(CellValue::Int(20))),
            ]
        );

        let left_row = store
            .table_at_mut(&Path::from("Left"))
            .expect("Left table")
            .add(vec![CellValue::Int(10)]);
        let right_row = store
            .table_at_mut(&Path::from("Right"))
            .expect("Right table")
            .add(vec![CellValue::Int(20)]);
        store
            .apply_batch(vec![left_row, right_row])
            .expect("insert referenced rows");

        assert!(check_law(&store, &compiled).is_ok());
    }

    #[test]
    fn consequent_equality_holds_when_bindings_match() {
        let t = Path::from("T");
        let mut store = Store::new();
        store.insert_table(
            t.clone(),
            Table::new(
                t.clone(),
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
        let table = store.table_at_mut(&t).expect("T table");
        let row = table.add(vec![CellValue::Int(5), CellValue::Int(5)]);
        store.apply_batch(vec![row]).expect("insert row");

        let law = ir::LawEntry {
            path: Path::from("T.eq"),
            law: ir::Law {
                variables: vec![
                    ColType::PrimType {
                        prim: PrimType::PrimInt,
                    },
                    ColType::PrimType {
                        prim: PrimType::PrimInt,
                    },
                ],
                antecedent: ir::Prop::Atom {
                    atom: ir::Atom {
                        table: t.clone(),
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
                consequent: ir::Prop::Eq {
                    left: ir::Term::Var { index: 0 },
                    right: ir::Term::Var { index: 1 },
                },
            },
        };

        let compiled = compile_law(&law).expect("compile law");
        assert!(check_law(&store, &compiled).is_ok());
    }

    #[test]
    fn consequent_equality_violation_surfaces_unsatisfied_eq() {
        let t = Path::from("T");
        let mut store = Store::new();
        store.insert_table(
            t.clone(),
            Table::new(
                t.clone(),
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
        let table = store.table_at_mut(&t).expect("T table");
        let row = table.add(vec![CellValue::Int(1), CellValue::Int(2)]);
        store.apply_batch(vec![row]).expect("insert row");

        let law = ir::LawEntry {
            path: Path::from("T.eq"),
            law: ir::Law {
                variables: vec![
                    ColType::PrimType {
                        prim: PrimType::PrimInt,
                    },
                    ColType::PrimType {
                        prim: PrimType::PrimInt,
                    },
                ],
                antecedent: ir::Prop::Atom {
                    atom: ir::Atom {
                        table: t.clone(),
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
                consequent: ir::Prop::Eq {
                    left: ir::Term::Var { index: 0 },
                    right: ir::Term::Var { index: 1 },
                },
            },
        };

        let compiled = compile_law(&law).expect("compile law");
        let violation = check_law(&store, &compiled).expect_err("eq should fail");
        match &violation.cause {
            ViolationCause::UnsatisfiedEq(eq) => {
                assert_eq!(eq.left, crate::solver::compile::CompTerm::Var(0));
                assert_eq!(eq.right, crate::solver::compile::CompTerm::Var(1));
            }
            other => panic!("expected UnsatisfiedEq, got {:?}", other),
        }
    }
}
