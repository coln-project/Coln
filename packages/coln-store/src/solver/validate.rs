use std::error::Error;
use std::fmt;
use tracing::debug;

use crate::{
    solver::{
        bind::{Binding, BoundValue, bind_law, eval_term},
        compile::{CompAtom, CompEq, CompProp, CompRule, CompVal},
        matcher::term_matches,
    },
    store::Store,
    table::CellValue,
};

/// Why a rule was violated at a given binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViolationCause {
    /// A required consequent atom was not present in the store.
    MissingAtom(CompAtom),
    /// An equality in the consequent did not hold.
    UnsatisfiedEq(CompEq),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleViolation {
    pub law: CompRule,
    pub cause: ViolationCause,
    pub binding: Binding,
}

impl fmt::Display for RuleViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.cause {
            ViolationCause::MissingAtom(atom) => write!(
                f,
                "rule {} violated: missing consequent atom in table {}",
                self.law, atom.table
            ),
            ViolationCause::UnsatisfiedEq(eq) => write!(
                f,
                "rule {} violated: equality {:?} = {:?} did not hold",
                self.law, eq.left, eq.right
            ),
        }
    }
}

impl Error for RuleViolation {}

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
/// caller can build a precise `RuleViolation`.
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

pub fn check_law(store: &Store, law: &CompRule) -> Result<(), Box<RuleViolation>> {
    let bindings = bind_law(store, law);
    debug!(law = %law.path, bindings = ?bindings, "checking law with bindings");

    for binding in bindings {
        if let Err(cause) = prop_holds(store, &binding, &law.consequent) {
            debug!(law = %law.path, cause = ?cause, "law violation detected");
            return Err(Box::new(RuleViolation {
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
        ir::{
            self, BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, Rule, RuleEntry,
            RuleVariant, Schema,
        },
        solver::compile::compile_law,
        table::{CellValue, Table},
    };

    fn int_ty() -> ColType {
        ColType::BuiltinTy {
            builtin_ty: BuiltinTy::BuiltinInt,
        }
    }

    fn int_col(name: &str) -> ColumnEntry {
        ColumnEntry {
            path: Path::from(name),
            col_type: int_ty(),
        }
    }

    fn int_schema(columns: &[&str]) -> Schema {
        Schema {
            entity_variant: EntityVariant::Table,
            columns: columns.iter().map(|name| int_col(name)).collect(),
            primary_key: None,
        }
    }

    fn enforced_rule(
        path: &str,
        var_types: Vec<ColType>,
        antecedents: Vec<ir::Prop>,
        consequents: Vec<ir::Prop>,
    ) -> RuleEntry {
        RuleEntry {
            path: Path::from(path),
            rule: Rule {
                rule_variant: RuleVariant::Enforced,
                var_names: (0..var_types.len())
                    .map(|index| Path::from(format!("v{index}")))
                    .collect(),
                var_types,
                antecedents,
                consequents,
            },
        }
    }

    #[test]
    fn true_antecedent_still_checks_consequent() {
        let g0_path = Path::from("G0");
        let mut store = Store::new();
        store.insert_table(
            g0_path.clone(),
            Table::new(g0_path.clone(), int_schema(&[])),
        );

        let law = enforced_rule(
            "G0.total",
            vec![],
            vec![],
            vec![ir::Prop::Atom {
                atom: ir::Atom {
                    entity: g0_path.clone(),
                    row_id: None,
                    values: vec![],
                },
            }],
        );

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
            Table::new(source.clone(), int_schema(&["c0"])),
        );
        store.insert_table(
            target.clone(),
            Table::new(target.clone(), int_schema(&["c0"])),
        );

        let mut txn = store.transaction();
        txn.add(&source, vec![CellValue::Int(7).into()])
            .expect("insert source row");
        txn.add(&target, vec![CellValue::Int(7).into()])
            .expect("insert target row");
        txn.commit().expect("commit matching rows");

        let law = enforced_rule(
            "Copy.total",
            vec![int_ty()],
            vec![ir::Prop::Atom {
                atom: ir::Atom {
                    entity: source,
                    row_id: None,
                    values: vec![ir::ValueEntry {
                        column: 0,
                        term: ir::Term::Var { index: 0 },
                    }],
                },
            }],
            vec![ir::Prop::Atom {
                atom: ir::Atom {
                    entity: target,
                    row_id: None,
                    values: vec![ir::ValueEntry {
                        column: 0,
                        term: ir::Term::Var { index: 0 },
                    }],
                },
            }],
        );

        let compiled = compile_law(&law).expect("compile law");
        assert!(check_law(&store, &compiled).is_ok());
    }

    #[test]
    fn foreign_key_law_fails_when_only_referencing_row_exists() {
        let left = Path::from("Left");
        let right = Path::from("Right");
        let link = Path::from("Link");
        let mut store = Store::new();
        store.insert_table(left.clone(), Table::new(left.clone(), int_schema(&["c0"])));
        store.insert_table(
            right.clone(),
            Table::new(right.clone(), int_schema(&["c0"])),
        );
        store.insert_table(
            link.clone(),
            Table::new(link.clone(), int_schema(&["c0", "c1"])),
        );

        let law = enforced_rule(
            "Link.foreignKeys",
            vec![int_ty(), int_ty()],
            vec![ir::Prop::Atom {
                atom: ir::Atom {
                    entity: link.clone(),
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
            }],
            vec![
                ir::Prop::Atom {
                    atom: ir::Atom {
                        entity: left.clone(),
                        row_id: None,
                        values: vec![ir::ValueEntry {
                            column: 0,
                            term: ir::Term::Var { index: 0 },
                        }],
                    },
                },
                ir::Prop::Atom {
                    atom: ir::Atom {
                        entity: right.clone(),
                        row_id: None,
                        values: vec![ir::ValueEntry {
                            column: 0,
                            term: ir::Term::Var { index: 1 },
                        }],
                    },
                },
            ],
        );

        let compiled = compile_law(&law).expect("compile law");

        let mut txn = store.transaction();
        txn.add(
            &link,
            vec![CellValue::Int(10).into(), CellValue::Int(20).into()],
        )
        .expect("insert referencing row");
        txn.commit().expect("commit referencing row");

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

        let mut txn = store.transaction();
        txn.add(&left, vec![CellValue::Int(10).into()])
            .expect("insert left row");
        txn.add(&right, vec![CellValue::Int(20).into()])
            .expect("insert right row");
        txn.commit().expect("commit referenced rows");

        assert!(check_law(&store, &compiled).is_ok());
    }

    #[test]
    fn consequent_equality_holds_when_bindings_match() {
        let t = Path::from("T");
        let mut store = Store::new();
        store.insert_table(t.clone(), Table::new(t.clone(), int_schema(&["c0", "c1"])));
        let mut txn = store.transaction();
        txn.add(&t, vec![CellValue::Int(5).into(), CellValue::Int(5).into()])
            .expect("insert row");
        txn.commit().expect("commit row");

        let law = enforced_rule(
            "T.eq",
            vec![int_ty(), int_ty()],
            vec![ir::Prop::Atom {
                atom: ir::Atom {
                    entity: t.clone(),
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
            }],
            vec![ir::Prop::Eq {
                left: ir::Term::Var { index: 0 },
                right: ir::Term::Var { index: 1 },
            }],
        );

        let compiled = compile_law(&law).expect("compile law");
        assert!(check_law(&store, &compiled).is_ok());
    }

    #[test]
    fn consequent_equality_violation_surfaces_unsatisfied_eq() {
        let t = Path::from("T");
        let mut store = Store::new();
        store.insert_table(t.clone(), Table::new(t.clone(), int_schema(&["c0", "c1"])));
        let mut txn = store.transaction();
        txn.add(&t, vec![CellValue::Int(1).into(), CellValue::Int(2).into()])
            .expect("insert row");
        txn.commit().expect("commit row");

        let law = enforced_rule(
            "T.eq",
            vec![int_ty(), int_ty()],
            vec![ir::Prop::Atom {
                atom: ir::Atom {
                    entity: t.clone(),
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
            }],
            vec![ir::Prop::Eq {
                left: ir::Term::Var { index: 0 },
                right: ir::Term::Var { index: 1 },
            }],
        );

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
