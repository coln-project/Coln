use std::{collections::HashSet, fmt};

use crate::ir::{self, Atom, Prop, RuleEntry, Term};

/// Errors raised while lowering an `ir::Rule` into the restricted solver form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    UnsupportedProp(String),
    UnsupportedTerm,
    InvalidVarIndex { index: i64, var_count: usize },
    InvalidColumnIndex { column: i64 },
}

/// A rule lowered into a small execution-oriented law form.
///
/// The IR stores antecedents and consequents as vectors of propositions; each
/// vector is interpreted as a conjunction here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompRule {
    pub path: ir::Path,
    pub vars: Vec<VarSpec>,
    pub antecedent: CompProp,
    pub consequent: CompProp,
    pub tables: Vec<ir::Path>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarSpec {
    pub index: usize,
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

/// Lower one parsed rule into the restricted solver representation.
///
/// This performs three main tasks:
/// - keeps table references in their source-level `ir::Path` form
/// - validates variable and column indices
/// - rejects IR forms not yet supported by the solver
pub fn compile_law(rule_entry: &RuleEntry) -> Result<CompRule, CompileError> {
    let path = rule_entry.path.clone();
    let vars = rule_entry
        .rule
        .var_types
        .clone()
        .into_iter()
        .enumerate()
        .map(|(index, ty)| VarSpec { index, ty })
        .collect::<Vec<_>>();

    let var_count = vars.len();
    let antecedent = compile_props(&rule_entry.rule.antecedents, var_count)?;
    let consequent = compile_props(&rule_entry.rule.consequents, var_count)?;

    let mut seen = HashSet::new();
    let mut tables = Vec::new();
    collect_atom_tables(&antecedent, &mut seen, &mut tables);
    collect_atom_tables(&consequent, &mut seen, &mut tables);

    Ok(CompRule {
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

/// Compile a vector of IR propositions as an implicit conjunction.
fn compile_props(props: &[Prop], var_count: usize) -> Result<CompProp, CompileError> {
    let children = props
        .iter()
        .map(|prop| compile_prop(prop, var_count))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CompProp::And(children))
}

/// Compile one IR proposition into a structured `CompProp`.
fn compile_prop(prop: &Prop, var_count: usize) -> Result<CompProp, CompileError> {
    match prop {
        Prop::Atom { atom } => Ok(CompProp::Atom(compile_atom(atom, var_count)?)),
        Prop::Eq { left, right } => Ok(CompProp::Eq(CompEq {
            left: compile_term(left, var_count)?,
            right: compile_term(right, var_count)?,
        })),
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
        table: atom.entity.clone(),
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
    }
}

impl fmt::Display for CompRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} := forall", DisplayPath(&self.path))?;
        for var in &self.vars {
            write!(
                f,
                " ({} : {})",
                var_name(var.index),
                DisplayColType(&var.ty)
            )?;
        }
        if self.vars.is_empty() {
            write!(f, " ")?;
        }
        write!(f, " => {} |- {}", self.antecedent, self.consequent)
    }
}

impl fmt::Display for CompProp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompProp::Atom(atom) => write!(f, "{atom}"),
            CompProp::Eq(eq) => write!(f, "{eq}"),
            CompProp::And(children) => {
                for (idx, child) in children.iter().enumerate() {
                    if idx > 0 {
                        write!(f, " /\\ ")?;
                    }
                    write!(f, "{child}")?;
                }
                Ok(())
            }
        }
    }
}

impl fmt::Display for CompAtom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(row_id) = &self.row_id {
            write!(f, "{row_id} @ ")?;
        }
        write!(f, "{} [", DisplayPath(&self.table))?;
        for (idx, value) in self.values.iter().enumerate() {
            if idx > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", value.term)?;
        }
        write!(f, "]")
    }
}

impl fmt::Display for CompEq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} = {}", self.left, self.right)
    }
}

impl fmt::Display for CompTerm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompTerm::Var(index) => write!(f, "{}", var_name(*index)),
            CompTerm::Lit(lit) => write!(f, "{}", DisplayLit(lit)),
        }
    }
}

struct DisplayLit<'a>(&'a ir::Lit);

impl fmt::Display for DisplayLit<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ir::Lit::Int { value } => write!(f, "{value}"),
            ir::Lit::String { value } => write!(f, "{value:?}"),
        }
    }
}

struct DisplayColType<'a>(&'a ir::ColType);

impl fmt::Display for DisplayColType<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ir::ColType::RowId { path } => write!(f, "{}", DisplayPath(path)),
            ir::ColType::BuiltinTy { builtin_ty } => write!(f, "{}", DisplayBuiltinTy(*builtin_ty)),
        }
    }
}

struct DisplayBuiltinTy(ir::BuiltinTy);

impl fmt::Display for DisplayBuiltinTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ir::BuiltinTy::BuiltinInt => write!(f, "int"),
            ir::BuiltinTy::BuiltinStr => write!(f, "string"),
        }
    }
}

struct DisplayPath<'a>(&'a ir::Path);

impl fmt::Display for DisplayPath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (idx, qname) in self.0.iter().enumerate() {
            if idx > 0 {
                write!(f, " .")?;
            }
            write!(f, "{}", display_qname(qname))?;
        }
        Ok(())
    }
}

fn display_qname(qname: &ir::QName) -> String {
    qname.join("/")
}

fn var_name(index: usize) -> String {
    match index {
        0..=25 => ((b'a' + index as u8) as char).to_string(),
        _ => format!("v{index}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{BuiltinTy, ColType, Path, Rule, RuleEntry, RuleVariant};

    fn int_ty() -> ColType {
        ColType::BuiltinTy {
            builtin_ty: BuiltinTy::BuiltinInt,
        }
    }

    fn row_id_ty(path: &str) -> ColType {
        ColType::RowId {
            path: Path::from(path),
        }
    }

    fn enforced_rule(
        path: &str,
        var_types: Vec<ColType>,
        antecedents: Vec<Prop>,
        consequents: Vec<Prop>,
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
    fn compiles_single_atom_law() {
        let law = enforced_rule(
            "T.total",
            vec![int_ty()],
            vec![Prop::Atom {
                atom: Atom {
                    entity: Path::from("T"),
                    row_id: None,
                    values: vec![ir::ValueEntry {
                        column: 0,
                        term: Term::Var { index: 0 },
                    }],
                },
            }],
            vec![Prop::Atom {
                atom: Atom {
                    entity: Path::from("T"),
                    row_id: None,
                    values: vec![ir::ValueEntry {
                        column: 0,
                        term: Term::Var { index: 0 },
                    }],
                },
            }],
        );

        let compiled = compile_law(&law).expect("compile law");
        assert!(matches!(compiled.antecedent, CompProp::And(_)));
        assert!(matches!(compiled.consequent, CompProp::And(_)));
        assert_eq!(compiled.tables.len(), 1);
        match &compiled.antecedent {
            CompProp::And(children) if matches!(children.as_slice(), [CompProp::Atom(_)]) => {
                let CompProp::Atom(atom) = &children[0] else {
                    unreachable!()
                };
                assert_eq!(atom.table, Path::from("T"));
                assert_eq!(atom.values[0].column_idx, 0);
                assert_eq!(atom.values[0].term, CompTerm::Var(0));
            }
            other => panic!("expected atom antecedent, got {:?}", other),
        }
    }

    #[test]
    fn displays_compiled_law_like_lowered_output() {
        let compiled = CompRule {
            path: Path::from("G.E.foreignKeys"),
            vars: vec![
                VarSpec {
                    index: 0,
                    ty: row_id_ty("Graphs"),
                },
                VarSpec {
                    index: 1,
                    ty: row_id_ty("G.V"),
                },
                VarSpec {
                    index: 2,
                    ty: row_id_ty("G.V"),
                },
            ],
            antecedent: CompProp::Atom(CompAtom {
                table: Path::from("G.E"),
                row_id: None,
                values: vec![
                    CompVal {
                        column_idx: 0,
                        term: CompTerm::Var(0),
                    },
                    CompVal {
                        column_idx: 1,
                        term: CompTerm::Var(1),
                    },
                    CompVal {
                        column_idx: 2,
                        term: CompTerm::Var(2),
                    },
                ],
            }),
            consequent: CompProp::And(vec![
                CompProp::Atom(CompAtom {
                    table: Path::from("G.V"),
                    row_id: Some(CompTerm::Var(1)),
                    values: vec![CompVal {
                        column_idx: 0,
                        term: CompTerm::Var(0),
                    }],
                }),
                CompProp::Atom(CompAtom {
                    table: Path::from("G.V"),
                    row_id: Some(CompTerm::Var(2)),
                    values: vec![CompVal {
                        column_idx: 0,
                        term: CompTerm::Var(0),
                    }],
                }),
            ]),
            tables: vec![Path::from("G.E"), Path::from("G.V")],
        };

        assert_eq!(
            compiled.to_string(),
            "G .E .foreignKeys := forall (a : Graphs) (b : G .V) (c : G .V) => G .E [a, b, c] |- b @ G .V [a] /\\ c @ G .V [a]"
        );
    }

    #[test]
    fn displays_compiled_equality_consequent() {
        let compiled = CompRule {
            path: Path::from("PathHom.empty"),
            vars: vec![
                VarSpec {
                    index: 0,
                    ty: row_id_ty("Path0.t"),
                },
                VarSpec {
                    index: 1,
                    ty: row_id_ty("Path1.t"),
                },
            ],
            antecedent: CompProp::Atom(CompAtom {
                table: Path::from("PathHom.t"),
                row_id: None,
                values: vec![
                    CompVal {
                        column_idx: 0,
                        term: CompTerm::Var(0),
                    },
                    CompVal {
                        column_idx: 1,
                        term: CompTerm::Var(1),
                    },
                ],
            }),
            consequent: CompProp::Eq(CompEq {
                left: CompTerm::Var(0),
                right: CompTerm::Var(1),
            }),
            tables: vec![Path::from("PathHom.t")],
        };

        assert_eq!(
            compiled.to_string(),
            "PathHom .empty := forall (a : Path0 .t) (b : Path1 .t) => PathHom .t [a, b] |- a = b"
        );
    }

    #[test]
    fn compiles_eq_in_antecedent() {
        let law = enforced_rule(
            "T.eq_antecedent",
            vec![int_ty(), int_ty()],
            vec![Prop::Eq {
                left: Term::Var { index: 0 },
                right: Term::Var { index: 1 },
            }],
            vec![Prop::Atom {
                atom: Atom {
                    entity: Path::from("T"),
                    row_id: None,
                    values: vec![],
                },
            }],
        );
        let compiled = compile_law(&law).expect("compile law");
        assert!(matches!(
            compiled.antecedent,
            CompProp::And(ref children) if matches!(children.as_slice(), [CompProp::Eq(_)])
        ));
    }

    #[test]
    fn compiles_consequent_equality() {
        let law = enforced_rule(
            "T.eq",
            vec![int_ty(), int_ty()],
            vec![Prop::Atom {
                atom: Atom {
                    entity: Path::from("T"),
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
            }],
            vec![Prop::Eq {
                left: Term::Var { index: 0 },
                right: Term::Var { index: 1 },
            }],
        );

        let compiled = compile_law(&law).expect("compile law");
        match compiled.consequent {
            CompProp::And(children) if matches!(children.as_slice(), [CompProp::Eq(_)]) => {
                let CompProp::Eq(CompEq { left, right }) = &children[0] else {
                    unreachable!()
                };
                assert_eq!(*left, CompTerm::Var(0));
                assert_eq!(*right, CompTerm::Var(1));
            }
            other => panic!("expected CompProp::Eq, got {:?}", other),
        }
        // Eq does not introduce new table references.
        assert_eq!(compiled.tables, vec![Path::from("T")]);
    }

    #[test]
    fn compiles_conjunction_of_atoms_and_eq() {
        let t = Path::from("T");
        let law = enforced_rule(
            "T.mixed",
            vec![int_ty(), int_ty()],
            vec![Prop::Atom {
                atom: Atom {
                    entity: t.clone(),
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
            }],
            vec![
                Prop::Atom {
                    atom: Atom {
                        entity: t.clone(),
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
        );

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
}
