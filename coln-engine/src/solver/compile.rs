use std::collections::HashSet;

use crate::ir::{self, Atom, LawEntry, Prop, Term};

/// Errors raised while lowering an `ir::Law` into the restricted solver form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    UnsupportedAntecedentProp,
    UnsupportedConsequentProp,
    UnsupportedTerm,
    InvalidVarIndex { index: i64, var_count: usize },
    InvalidColumnIndex { column: i64 },
}

/// A law lowered into a small execution-oriented form.
///
/// This keeps only the subset currently supported by the solver:
/// antecedent/consequent atoms over variables and literals.
#[derive(Debug, Clone)]
pub struct CompLaw {
    pub path: ir::Path,
    pub vars: Vec<VarSpec>,
    pub antecedent: Vec<CompAtom>,
    pub consequent: Vec<CompAtom>,
    pub tables: Vec<ir::Path>,
}

#[derive(Debug, Clone)]
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
    let antecedent = compile_prop_atoms(&law_entry.law.antecedent, var_count, true)?;
    let consequent = compile_prop_atoms(&law_entry.law.consequent, var_count, false)?;

    let mut seen = HashSet::new();
    let mut tables = Vec::new();
    for path in antecedent
        .iter()
        .chain(consequent.iter())
        .map(|atom| atom.table.clone())
    {
        if seen.insert(path.clone()) {
            tables.push(path);
        }
    }

    Ok(CompLaw {
        path,
        vars,
        antecedent,
        consequent,
        tables,
    })
}

fn compile_prop_atoms(
    prop: &Prop,
    var_count: usize,
    is_antecedent: bool,
) -> Result<Vec<CompAtom>, CompileError> {
    match prop {
        Prop::Atom { atom } => Ok(vec![compile_atom(atom, var_count)?]),
        Prop::And { props } => props
            .into_iter()
            .map(|prop| match prop {
                Prop::Atom { atom } => compile_atom(atom, var_count),
                _ if is_antecedent => Err(CompileError::UnsupportedAntecedentProp),
                _ => Err(CompileError::UnsupportedConsequentProp),
            })
            .collect(),
        _ if is_antecedent => Err(CompileError::UnsupportedAntecedentProp),
        _ => Err(CompileError::UnsupportedConsequentProp),
    }
}

fn compile_atom(atom: &Atom, var_count: usize) -> Result<CompAtom, CompileError> {
    let row_id = atom
        .row_id
        .as_ref()
        .map(|term| compile_term(&term, var_count))
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
        assert_eq!(compiled.antecedent.len(), 1);
        assert_eq!(compiled.consequent.len(), 1);
        assert_eq!(compiled.tables.len(), 1);
        assert_eq!(compiled.antecedent[0].table, Path::from("T"));
        assert_eq!(compiled.antecedent[0].values[0].column_idx, 0);
        assert_eq!(compiled.antecedent[0].values[0].term, CompTerm::Var(0));
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
