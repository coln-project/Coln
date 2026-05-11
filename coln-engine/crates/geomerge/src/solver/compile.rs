use std::collections::HashSet;

use crate::ir::{self, Atom, LawEntry, Prop, Term};

/// Errors raised while lowering an `ir::Law` into the restricted solver form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    UnsupportedProp(String),
    UnsupportedTerm,
    InvalidVarIndex { index: i64, var_count: usize },
    InvalidColumnIndex { column: i64 },
}

/// A law lowered into a small execution-oriented form.
///
/// Both sides are geometric formulas represented as `CompProp`.
/// FIXME: Currently only supports conjunction and equality in consequent
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompLaw {
    pub path: ir::Path,
    pub vars: Vec<VarSpec>,
    pub antecedent: CompProp,
    pub consequent: CompProp,
    pub tables: Vec<ir::Path>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarSpec {
    pub index: usize,
    // TODO consider not using ir
    pub ty: ir::ColType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompAtom {
    pub table: ir::Path,
    pub row_id: Option<CompTerm>,
    pub values: Vec<CompVal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompVal {
    /// Zero-based schema column index.
    pub column_idx: usize,
    /// Term that must match the cell stored at `column_idx`.
    pub term: CompTerm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompEq {
    pub left: CompTerm,
    pub right: CompTerm,
}

/// Structured proposition used for a law.
///
/// The shape mirrors `ir::Prop` but is restricted to variants the solver can
/// compile today. Additional variants (e.g. `Or`) can be added later without
/// changing the surrounding data model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompProp {
    Atom(CompAtom),
    Eq(CompEq),
    And(Vec<CompProp>),
}

// `Proj` and `Cons` are excluded for now
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompTerm {
    Var(usize),
    Lit(ir::Lit),
}

/// Lower one parsed law into the restricted solver representation.
///
/// This performs three main tasks:
/// - keeps table references in their source-level `ir::Path` form
/// - validates variable and column indices
/// - rejects IR forms not yet supported by the solver
pub fn compile_law(law_entry: &LawEntry) -> Result<CompLaw, CompileError> {
    let path = law_entry.path.clone();
    let vars = law_entry
        .law
        .variables
        .clone()
        .into_iter()
        .enumerate()
        .map(|(index, ty)| VarSpec { index, ty })
        .collect::<Vec<_>>();

    let var_count = vars.len();
    let antecedent = compile_prop(&law_entry.law.antecedent, var_count)?;
    let consequent = compile_prop(&law_entry.law.consequent, var_count)?;

    let mut seen = HashSet::new();
    let mut tables = Vec::new();
    collect_atom_tables(&antecedent, &mut seen, &mut tables);
    collect_atom_tables(&consequent, &mut seen, &mut tables);

    Ok(CompLaw {
        path,
        vars,
        antecedent,
        consequent,
        tables,
    })
}

/// Walk a `CompProp`, appending each atom's table path to `tables` on first
/// occurrence (tracked via `seen`).
fn collect_atom_tables(prop: &CompProp, seen: &mut HashSet<ir::Path>, tables: &mut Vec<ir::Path>) {
    match prop {
        CompProp::Atom(atom) => {
            if seen.insert(atom.table.clone()) {
                tables.push(atom.table.clone());
            }
        }
        CompProp::Eq(_) => {}
        CompProp::And(children) => {
            for child in children {
                collect_atom_tables(child, seen, tables);
            }
        }
    }
}

/// Compile a geometric formula into a structured `CompProp`.
///
/// Both the antecedent and consequent sides accept
/// the same variants today (`Atom`, `Eq`, `And`); side-specific runtime
/// semantics live in `bind.rs` (antecedent) and `validate.rs` (consequent).
fn compile_prop(prop: &Prop, var_count: usize) -> Result<CompProp, CompileError> {
    match prop {
        Prop::Atom { atom } => Ok(CompProp::Atom(compile_atom(atom, var_count)?)),
        Prop::Eq { left, right } => Ok(CompProp::Eq(CompEq {
            left: compile_term(left, var_count)?,
            right: compile_term(right, var_count)?,
        })),
        Prop::And { props } => {
            let children = props
                .iter()
                .map(|p| compile_prop(p, var_count))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(CompProp::And(children))
        }
        _ => Err(CompileError::UnsupportedProp(format!("{:?}", prop))),
    }
}

fn compile_atom(atom: &Atom, var_count: usize) -> Result<CompAtom, CompileError> {
    let row_id = atom
        .row_id
        .as_ref()
        .map(|term| compile_term(term, var_count))
        .transpose()?;

    let columns = atom
        .values
        .clone()
        .into_iter()
        .map(|value| {
            Ok(CompVal {
                column_idx: usize::try_from(value.column).map_err(|_| {
                    CompileError::InvalidColumnIndex {
                        column: value.column,
                    }
                })?,
                term: compile_term(&value.term, var_count)?,
            })
        })
        .collect::<Result<Vec<_>, CompileError>>()?;

    Ok(CompAtom {
        table: atom.table.clone(),
        row_id,
        values: columns,
    })
}

fn compile_term(term: &Term, var_count: usize) -> Result<CompTerm, CompileError> {
    match term {
        Term::Var { index } => {
            let index = usize::try_from(*index).map_err(|_| CompileError::InvalidVarIndex {
                index: *index,
                var_count,
            })?;
            if index >= var_count {
                return Err(CompileError::InvalidVarIndex {
                    index: index as i64,
                    var_count,
                });
            }
            Ok(CompTerm::Var(index))
        }
        Term::Lit { lit } => Ok(CompTerm::Lit(lit.clone())),
        Term::Proj { .. } | Term::Cons { .. } => Err(CompileError::UnsupportedTerm),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ColType, Path, PrimType};

    #[test]
    fn compiles_single_atom_law() {
        let law = LawEntry {
            path: Path::from("T.total"),
            law: ir::Law {
                variables: vec![ColType::PrimType {
                    prim: PrimType::PrimInt,
                }],
                antecedent: Prop::Atom {
                    atom: Atom {
                        table: Path::from("T"),
                        row_id: None,
                        values: vec![ir::ValueEntry {
                            column: 0,
                            term: Term::Var { index: 0 },
                        }],
                    },
                },
                consequent: Prop::Atom {
                    atom: Atom {
                        table: Path::from("T"),
                        row_id: None,
                        values: vec![ir::ValueEntry {
                            column: 0,
                            term: Term::Var { index: 0 },
                        }],
                    },
                },
            },
        };

        let compiled = compile_law(&law).expect("compile law");
        assert!(matches!(compiled.antecedent, CompProp::Atom(_)));
        assert!(matches!(compiled.consequent, CompProp::Atom(_)));
        assert_eq!(compiled.tables.len(), 1);
        match &compiled.antecedent {
            CompProp::Atom(atom) => {
                assert_eq!(atom.table, Path::from("T"));
                assert_eq!(atom.values[0].column_idx, 0);
                assert_eq!(atom.values[0].term, CompTerm::Var(0));
            }
            other => panic!("expected atom antecedent, got {:?}", other),
        }
    }

    #[test]
    fn compiles_eq_in_antecedent() {
        let law = LawEntry {
            path: Path::from("T.eq_antecedent"),
            law: ir::Law {
                variables: vec![
                    ColType::PrimType {
                        prim: PrimType::PrimInt,
                    },
                    ColType::PrimType {
                        prim: PrimType::PrimInt,
                    },
                ],
                antecedent: Prop::Eq {
                    left: Term::Var { index: 0 },
                    right: Term::Var { index: 1 },
                },
                consequent: Prop::Atom {
                    atom: Atom {
                        table: Path::from("T"),
                        row_id: None,
                        values: vec![],
                    },
                },
            },
        };
        let compiled = compile_law(&law).expect("compile law");
        assert!(matches!(compiled.antecedent, CompProp::Eq(_)));
    }

    #[test]
    fn compiles_consequent_equality() {
        let law = LawEntry {
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
                antecedent: Prop::Atom {
                    atom: Atom {
                        table: Path::from("T"),
                        row_id: None,
                        values: vec![
                            ir::ValueEntry {
                                column: 0,
                                term: Term::Var { index: 0 },
                            },
                            ir::ValueEntry {
                                column: 1,
                                term: Term::Var { index: 1 },
                            },
                        ],
                    },
                },
                consequent: Prop::Eq {
                    left: Term::Var { index: 0 },
                    right: Term::Var { index: 1 },
                },
            },
        };

        let compiled = compile_law(&law).expect("compile law");
        match compiled.consequent {
            CompProp::Eq(CompEq { left, right }) => {
                assert_eq!(left, CompTerm::Var(0));
                assert_eq!(right, CompTerm::Var(1));
            }
            other => panic!("expected CompProp::Eq, got {:?}", other),
        }
        // Eq does not introduce new table references.
        assert_eq!(compiled.tables, vec![Path::from("T")]);
    }

    #[test]
    fn compiles_conjunction_of_atoms_and_eq() {
        let t = Path::from("T");
        let law = LawEntry {
            path: Path::from("T.mixed"),
            law: ir::Law {
                variables: vec![
                    ColType::PrimType {
                        prim: PrimType::PrimInt,
                    },
                    ColType::PrimType {
                        prim: PrimType::PrimInt,
                    },
                ],
                antecedent: Prop::Atom {
                    atom: Atom {
                        table: t.clone(),
                        row_id: None,
                        values: vec![
                            ir::ValueEntry {
                                column: 0,
                                term: Term::Var { index: 0 },
                            },
                            ir::ValueEntry {
                                column: 1,
                                term: Term::Var { index: 1 },
                            },
                        ],
                    },
                },
                consequent: Prop::And {
                    props: vec![
                        Prop::Atom {
                            atom: Atom {
                                table: t.clone(),
                                row_id: None,
                                values: vec![ir::ValueEntry {
                                    column: 0,
                                    term: Term::Var { index: 0 },
                                }],
                            },
                        },
                        Prop::Eq {
                            left: Term::Var { index: 0 },
                            right: Term::Var { index: 1 },
                        },
                    ],
                },
            },
        };

        let compiled = compile_law(&law).expect("compile law");
        match &compiled.consequent {
            CompProp::And(children) => {
                assert_eq!(children.len(), 2);
                assert!(matches!(children[0], CompProp::Atom(_)));
                assert!(matches!(children[1], CompProp::Eq(_)));
            }
            other => panic!("expected CompProp::And, got {:?}", other),
        }
    }

    #[test]
    fn rejects_proj_terms() {
        let law = LawEntry {
            path: Path::from("bad"),
            law: ir::Law {
                variables: vec![ColType::Tuple { fields: vec![] }],
                antecedent: Prop::Atom {
                    atom: Atom {
                        table: Path::from("T"),
                        row_id: Some(Term::Proj {
                            term: Box::new(Term::Var { index: 0 }),
                            field: vec!["x".to_string()],
                        }),
                        values: vec![],
                    },
                },
                consequent: Prop::Atom {
                    atom: Atom {
                        table: Path::from("T"),
                        row_id: None,
                        values: vec![],
                    },
                },
            },
        };

        let err = compile_law(&law).unwrap_err();
        assert_eq!(err, CompileError::UnsupportedTerm);
    }
}
