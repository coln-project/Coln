use std::collections::HashSet;

use crate::{
    ir::{self, Atom, LawEntry, Prop, Term},
    store::Store,
    table::TableOid,
};

/// Errors raised while lowering an `ir::Law` into the restricted solver form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    UnknownTable { path: ir::Path },
    UnsupportedAntecedentProp,
    UnsupportedConsequentProp,
    UnsupportedTerm,
    InvalidVarIndex { index: i64, var_count: usize },
    InvalidColumnIndex { column: i64 },
}

/// A law lowered into a small execution-oriented form.
///
/// This keeps only the subset currently supported by the solver:
/// antecedent/consequent atoms over variables and literals, with table paths
/// already resolved to concrete [`TableOid`]s.
#[derive(Debug)]
pub struct CompLaw {
    pub path: ir::Path,
    pub vars: Vec<VarSpec>,
    pub antecedent: Vec<CompAtom>,
    pub consequent: Vec<CompAtom>,
    pub tables: Vec<TableOid>,
}

#[derive(Debug)]
pub struct VarSpec {
    pub index: usize,
    // TODO consider not using ir
    pub ty: ir::ColType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompAtom {
    pub table: TableOid,
    pub row_id: Option<CompTerm>,
    pub columns: Vec<CompCol>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompCol {
    /// Zero-based schema column index.
    pub column_idx: usize,
    /// Term that must match the cell stored at `column_idx`.
    pub term: CompTerm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lit {
    LInt(i64),
    LString(String),
}

// `Proj` and `Cons` are excluded for now
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompTerm {
    Var(usize),
    Lit(Lit),
}

/// Lower one parsed law into the restricted solver representation.
///
/// This performs three main tasks:
/// - resolves atom table paths through the [`Store`]
/// - validates variable and column indices
/// - rejects IR forms not yet supported by the solver
pub fn compile_law(store: &Store, law_entry: &LawEntry) -> Result<CompLaw, CompileError> {
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
    let antecedent = compile_prop_atoms(store, &law_entry.law.antecedent, var_count, true)?;
    let consequent = compile_prop_atoms(store, &law_entry.law.consequent, var_count, false)?;

    let mut seen = HashSet::new();
    let mut tables = Vec::new();
    for oid in antecedent
        .iter()
        .chain(consequent.iter())
        .map(|atom| atom.table)
    {
        if seen.insert(oid) {
            tables.push(oid);
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
    store: &Store,
    prop: &Prop,
    var_count: usize,
    is_antecedent: bool,
) -> Result<Vec<CompAtom>, CompileError> {
    match prop {
        Prop::Atom { atom } => Ok(vec![compile_atom(store, atom, var_count)?]),
        Prop::And { props } => props
            .into_iter()
            .map(|prop| match prop {
                Prop::Atom { atom } => compile_atom(store, atom, var_count),
                _ if is_antecedent => Err(CompileError::UnsupportedAntecedentProp),
                _ => Err(CompileError::UnsupportedConsequentProp),
            })
            .collect(),
        _ if is_antecedent => Err(CompileError::UnsupportedAntecedentProp),
        _ => Err(CompileError::UnsupportedConsequentProp),
    }
}

fn compile_atom(store: &Store, atom: &Atom, var_count: usize) -> Result<CompAtom, CompileError> {
    let table = store
        .resolve_table(&atom.table)
        .ok_or_else(|| CompileError::UnknownTable {
            path: atom.table.clone(),
        })?;

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
            Ok(CompCol {
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
        table,
        row_id,
        columns,
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
        Term::Lit { lit } => Ok(CompTerm::Lit(match lit {
            ir::Lit::Int { value } => Lit::LInt(*value),
            ir::Lit::String { value } => Lit::LString(value.clone()),
        })),
        Term::Proj { .. } | Term::Cons { .. } => Err(CompileError::UnsupportedTerm),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ir::{ColType, FlatTheory, Path, PrimType, Schema, TableEntry},
        table::Table,
    };

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

        let theory = FlatTheory {
            tables: vec![TableEntry {
                path: Path::from("T"),
                table: Schema {
                    columns: vec![ColType::PrimType {
                        prim: PrimType::PrimInt,
                    }],
                    primary_key: None,
                },
            }],
            laws: vec![law],
        };
        let store = Store::from_theory(theory);

        let compiled = compile_law(&store, &store.laws()[0]).expect("compile law");
        assert_eq!(compiled.antecedent.len(), 1);
        assert_eq!(compiled.consequent.len(), 1);
        assert_eq!(compiled.tables.len(), 1);
        assert_eq!(compiled.antecedent[0].columns[0].column_idx, 0);
        assert_eq!(compiled.antecedent[0].columns[0].term, CompTerm::Var(0));
    }

    #[test]
    fn rejects_proj_terms() {
        let mut store = Store::new();

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

        let oid = store.insert_table(
            Path::from("T"),
            Table::new(
                Path::from("T"),
                Schema {
                    columns: vec![],
                    primary_key: None,
                },
            ),
        );
        assert_eq!(oid, 0);

        let err = compile_law(&store, &law).unwrap_err();
        assert_eq!(err, CompileError::UnsupportedTerm);
    }
}
