use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use crate::{
    ir,
    solver::compile::{CompAtom, CompEq, CompLaw, CompProp, CompTerm},
};

/// Convert a compiled Coln law into an experimental Datalog constraint program.
///
/// The generated program is intended as a comparison target for Datalog-based
/// law checking. It does not replace the current solver. Table predicates use
/// `rid` as their first field, followed by `c0`, `c1`, and so on.
pub fn convert_law(law: &CompLaw) -> Result<DatalogProgram, ConvertError> {
    let mut converter = Converter::new(law);
    converter.convert()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatalogProgram {
    pub declarations: Vec<DatalogPredicate>,
    pub rules: Vec<DatalogRule>,
}

impl fmt::Display for DatalogProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for declaration in &self.declarations {
            writeln!(f, "{declaration} :- .")?;
        }

        if !self.declarations.is_empty() && !self.rules.is_empty() {
            writeln!(f)?;
        }

        for (idx, rule) in self.rules.iter().enumerate() {
            if idx > 0 {
                writeln!(f)?;
            }
            write!(f, "{rule}")?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatalogRule {
    pub head: DatalogPredicate,
    pub body: Vec<DatalogLiteral>,
}

impl fmt::Display for DatalogRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let body = self
            .body
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "{} :- {}.", self.head, body)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatalogPredicate {
    pub name: String,
    pub fields: Vec<String>,
}

impl fmt::Display for DatalogPredicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.name, self.fields.join(", "))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatalogLiteral {
    Predicate(DatalogPredicate),
    NotPredicate(DatalogPredicate),
    Comparison {
        left: String,
        operator: ComparisonOp,
        right: String,
    },
}

impl fmt::Display for DatalogLiteral {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatalogLiteral::Predicate(predicate) => write!(f, "{predicate}"),
            DatalogLiteral::NotPredicate(predicate) => write!(f, "not {predicate}"),
            DatalogLiteral::Comparison {
                left,
                operator,
                right,
            } => {
                write!(f, "{left} {operator} {right}")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComparisonOp {
    Eq,
    NotEq,
}

impl fmt::Display for ComparisonOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ComparisonOp::Eq => write!(f, "=="),
            ComparisonOp::NotEq => write!(f, "!="),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertError {
    MissingTableArity { table: String },
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvertError::MissingTableArity { table } => {
                write!(f, "missing Datalog arity for table `{table}`")
            }
        }
    }
}

impl Error for ConvertError {}

struct Converter<'a> {
    law: &'a CompLaw,
    prefix: String,
    table_arities: BTreeMap<String, usize>,
    scratch: usize,
}

impl<'a> Converter<'a> {
    fn new(law: &'a CompLaw) -> Self {
        let mut converter = Self {
            law,
            prefix: sanitize_identifier(&law.path.to_string()),
            table_arities: BTreeMap::new(),
            scratch: 0,
        };
        converter.collect_table_arities(&law.antecedent);
        converter.collect_table_arities(&law.consequent);
        converter
    }

    fn convert(&mut self) -> Result<DatalogProgram, ConvertError> {
        let declarations = self
            .table_arities
            .iter()
            .map(|(table, arity)| DatalogPredicate {
                name: table.clone(),
                fields: table_fields(*arity),
            })
            .collect::<Vec<_>>();
        let mut rules = Vec::new();
        let head = self.law_head_fields();
        let antecedent_body = self.convert_prop_body(&self.law.antecedent)?;
        rules.push(DatalogRule {
            head: DatalogPredicate {
                name: format!("{}_ante", self.prefix),
                fields: head.clone(),
            },
            body: antecedent_body,
        });

        for (idx, atom) in consequent_atoms(&self.law.consequent)
            .into_iter()
            .enumerate()
        {
            let fields = atom_head_fields(atom);
            let body = self.convert_atom_body(atom)?;
            rules.push(DatalogRule {
                head: DatalogPredicate {
                    name: format!("{}_cons_{idx}", self.prefix),
                    fields,
                },
                body,
            });
        }

        for (idx, atom) in consequent_atoms(&self.law.consequent)
            .into_iter()
            .enumerate()
        {
            rules.push(DatalogRule {
                head: DatalogPredicate {
                    name: format!("{}_violations", self.prefix),
                    fields: head.clone(),
                },
                body: vec![
                    DatalogLiteral::Predicate(DatalogPredicate {
                        name: format!("{}_ante", self.prefix),
                        fields: head.clone(),
                    }),
                    DatalogLiteral::NotPredicate(DatalogPredicate {
                        name: format!("{}_cons_{idx}", self.prefix),
                        fields: atom_head_fields(atom),
                    }),
                ],
            });
        }

        for eq in consequent_eqs(&self.law.consequent) {
            rules.push(DatalogRule {
                head: DatalogPredicate {
                    name: format!("{}_violations", self.prefix),
                    fields: head.clone(),
                },
                body: vec![
                    DatalogLiteral::Predicate(DatalogPredicate {
                        name: format!("{}_ante", self.prefix),
                        fields: head.clone(),
                    }),
                    DatalogLiteral::Comparison {
                        left: convert_term(&eq.left),
                        operator: ComparisonOp::NotEq,
                        right: convert_term(&eq.right),
                    },
                ],
            });
        }

        Ok(DatalogProgram {
            declarations,
            rules,
        })
    }

    fn collect_table_arities(&mut self, prop: &CompProp) {
        match prop {
            CompProp::Atom(atom) => {
                let table = sanitize_identifier(&atom.table.to_string());
                let arity = atom
                    .values
                    .iter()
                    .map(|value| value.column_idx + 2)
                    .max()
                    .unwrap_or(1);
                self.table_arities
                    .entry(table)
                    .and_modify(|current| *current = (*current).max(arity))
                    .or_insert(arity);
            }
            CompProp::Eq(_) => {}
            CompProp::And(children) => {
                for child in children {
                    self.collect_table_arities(child);
                }
            }
        }
    }

    fn law_head_fields(&self) -> Vec<String> {
        if self.law.vars.is_empty() {
            return vec!["_unit".to_string()];
        }
        (0..self.law.vars.len()).map(var_name).collect::<Vec<_>>()
    }

    fn convert_prop_body(&mut self, prop: &CompProp) -> Result<Vec<DatalogLiteral>, ConvertError> {
        let mut parts = Vec::new();
        self.push_prop_body(prop, &mut parts)?;
        if parts.is_empty() {
            Ok(vec![DatalogLiteral::Comparison {
                left: "0".to_string(),
                operator: ComparisonOp::Eq,
                right: "0".to_string(),
            }])
        } else {
            Ok(parts)
        }
    }

    fn push_prop_body(
        &mut self,
        prop: &CompProp,
        parts: &mut Vec<DatalogLiteral>,
    ) -> Result<(), ConvertError> {
        match prop {
            CompProp::Atom(atom) => parts.extend(self.convert_atom_body(atom)?),
            CompProp::Eq(eq) => parts.push(convert_eq(eq, ComparisonOp::Eq)),
            CompProp::And(children) => {
                for child in children {
                    self.push_prop_body(child, parts)?;
                }
            }
        }
        Ok(())
    }

    fn convert_atom_body(&mut self, atom: &CompAtom) -> Result<Vec<DatalogLiteral>, ConvertError> {
        let table = sanitize_identifier(&atom.table.to_string());
        let arity = self.table_arities.get(&table).copied().ok_or_else(|| {
            ConvertError::MissingTableArity {
                table: table.clone(),
            }
        })?;
        let mut fields = Vec::with_capacity(arity);
        let mut filters = Vec::new();

        fields.push(self.convert_atom_field("rid", atom.row_id.as_ref(), &mut filters));
        for column_idx in 0..(arity - 1) {
            let source = format!("c{column_idx}");
            let term = atom
                .values
                .iter()
                .find(|value| value.column_idx == column_idx)
                .map(|value| &value.term);
            fields.push(self.convert_atom_field(&source, term, &mut filters));
        }

        let mut body = vec![DatalogLiteral::Predicate(DatalogPredicate {
            name: table,
            fields,
        })];
        body.extend(filters);
        Ok(body)
    }

    fn convert_atom_field(
        &mut self,
        source: &str,
        term: Option<&CompTerm>,
        filters: &mut Vec<DatalogLiteral>,
    ) -> String {
        match term {
            Some(CompTerm::Var(index)) => format!("{}={source}", var_name(*index)),
            Some(CompTerm::Lit(lit)) => {
                let scratch = self.next_scratch(source);
                filters.push(DatalogLiteral::Comparison {
                    left: scratch.clone(),
                    operator: ComparisonOp::Eq,
                    right: convert_lit(lit),
                });
                format!("{scratch}={source}")
            }
            None => format!("{}={source}", self.next_scratch(source)),
        }
    }

    fn next_scratch(&mut self, source: &str) -> String {
        let scratch = format!("_{}_{}", sanitize_identifier(source), self.scratch);
        self.scratch += 1;
        scratch
    }
}

fn consequent_atoms(prop: &CompProp) -> Vec<&CompAtom> {
    let mut atoms = Vec::new();
    collect_consequent_atoms(prop, &mut atoms);
    atoms
}

fn collect_consequent_atoms<'a>(prop: &'a CompProp, atoms: &mut Vec<&'a CompAtom>) {
    match prop {
        CompProp::Atom(atom) => atoms.push(atom),
        CompProp::Eq(_) => {}
        CompProp::And(children) => {
            for child in children {
                collect_consequent_atoms(child, atoms);
            }
        }
    }
}

fn consequent_eqs(prop: &CompProp) -> Vec<&CompEq> {
    let mut eqs = Vec::new();
    collect_consequent_eqs(prop, &mut eqs);
    eqs
}

fn collect_consequent_eqs<'a>(prop: &'a CompProp, eqs: &mut Vec<&'a CompEq>) {
    match prop {
        CompProp::Atom(_) => {}
        CompProp::Eq(eq) => eqs.push(eq),
        CompProp::And(children) => {
            for child in children {
                collect_consequent_eqs(child, eqs);
            }
        }
    }
}

fn atom_head_fields(atom: &CompAtom) -> Vec<String> {
    let mut vars = BTreeSet::new();
    collect_atom_vars(atom, &mut vars);
    if vars.is_empty() {
        return vec!["_unit".to_string()];
    }
    vars.into_iter().map(var_name).collect::<Vec<_>>()
}

fn collect_atom_vars(atom: &CompAtom, vars: &mut BTreeSet<usize>) {
    if let Some(CompTerm::Var(index)) = atom.row_id {
        vars.insert(index);
    }
    for value in &atom.values {
        if let CompTerm::Var(index) = value.term {
            vars.insert(index);
        }
    }
}

fn convert_eq(eq: &CompEq, operator: ComparisonOp) -> DatalogLiteral {
    DatalogLiteral::Comparison {
        left: convert_term(&eq.left),
        operator,
        right: convert_term(&eq.right),
    }
}

fn convert_term(term: &CompTerm) -> String {
    match term {
        CompTerm::Var(index) => var_name(*index),
        CompTerm::Lit(lit) => convert_lit(lit),
    }
}

fn convert_lit(lit: &ir::Lit) -> String {
    match lit {
        ir::Lit::Int { value } => value.to_string(),
        ir::Lit::String { value } => format!("\"{}\"", value.replace('"', "")),
    }
}

fn var_name(index: usize) -> String {
    format!("x{index}")
}

fn table_fields(arity: usize) -> Vec<String> {
    let mut fields = Vec::with_capacity(arity);
    fields.push("rid".to_string());
    fields.extend((0..arity.saturating_sub(1)).map(|idx| format!("c{idx}")));
    fields
}

fn sanitize_identifier(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }

    let starts_valid = out
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_');
    if out.is_empty() || !starts_valid {
        out.insert_str(0, "p_");
    }
    out
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;
    use crate::{
        ir::{Atom, ColType, FlatTheory, Law, LawEntry, Path, PrimType, Prop, Term, ValueEntry},
        solver::compile::compile_law,
    };

    fn entity(path: &str) -> ColType {
        ColType::EntityType {
            path: Path::from(path),
        }
    }

    #[test]
    fn converts_martin_foreign_key_shape() {
        let law = LawEntry {
            path: Path::from("f.foreignKeys"),
            law: Law {
                variables: vec![entity("X"), entity("Y")],
                antecedent: Prop::Atom {
                    atom: Atom {
                        table: Path::from("f"),
                        row_id: None,
                        values: vec![
                            ValueEntry {
                                column: 0,
                                term: Term::Var { index: 0 },
                            },
                            ValueEntry {
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
                                table: Path::from("X"),
                                row_id: Some(Term::Var { index: 0 }),
                                values: vec![],
                            },
                        },
                        Prop::Atom {
                            atom: Atom {
                                table: Path::from("Y"),
                                row_id: Some(Term::Var { index: 1 }),
                                values: vec![],
                            },
                        },
                    ],
                },
            },
        };

        let compiled = compile_law(&law).expect("compile law");
        let program = convert_law(&compiled).expect("convert law");

        assert_eq!(program.declarations.len(), 3);
        assert_eq!(program.rules.len(), 5);
        assert_eq!(program.rules[0].head.name, "f_foreignKeys_ante");

        let datalog = program.to_string();

        assert_eq!(
            datalog,
            [
                "X(rid) :- .",
                "Y(rid) :- .",
                "f(rid, c0, c1) :- .",
                "",
                "f_foreignKeys_ante(x0, x1) :- f(_rid_0=rid, x0=c0, x1=c1).",
                "f_foreignKeys_cons_0(x0) :- X(x0=rid).",
                "f_foreignKeys_cons_1(x1) :- Y(x1=rid).",
                "f_foreignKeys_violations(x0, x1) :- f_foreignKeys_ante(x0, x1), not f_foreignKeys_cons_0(x0).",
                "f_foreignKeys_violations(x0, x1) :- f_foreignKeys_ante(x0, x1), not f_foreignKeys_cons_1(x1).",
            ]
            .join("\n")
        );
    }

    #[test]
    fn converts_consequent_equality_as_violation_filter() {
        let law = LawEntry {
            path: Path::from("T.same"),
            law: Law {
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
                            ValueEntry {
                                column: 0,
                                term: Term::Var { index: 0 },
                            },
                            ValueEntry {
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
        let datalog = convert_law(&compiled).expect("convert law").to_string();

        assert!(datalog.contains("T_same_ante(x0, x1) :- T(_rid_0=rid, x0=c0, x1=c1)."));
        assert!(datalog.contains("T_same_violations(x0, x1) :- T_same_ante(x0, x1), x0 != x1."));
    }

    #[test]
    fn converts_paths_fixture_laws() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join("paths.json");
        let json = fs::read_to_string(path).expect("read paths fixture");
        let theory: FlatTheory = serde_json::from_str(&json).expect("parse paths fixture");

        let converted = theory
            .laws
            .iter()
            .map(|law| {
                let compiled = compile_law(law).expect("compile law");
                convert_law(&compiled).map(|program| program.to_string())
            })
            .collect::<Result<Vec<_>, _>>()
            .expect("convert fixture laws");

        assert_eq!(converted.len(), theory.laws.len());
        assert!(
            converted
                .iter()
                .any(|program| program.contains("G_E_foreignKeys_violations"))
        );
    }

    #[test]
    fn missing_table_arity_returns_error() {
        let law = LawEntry {
            path: Path::from("T.total"),
            law: Law {
                variables: vec![entity("T")],
                antecedent: Prop::Atom {
                    atom: Atom {
                        table: Path::from("T"),
                        row_id: Some(Term::Var { index: 0 }),
                        values: vec![],
                    },
                },
                consequent: Prop::Atom {
                    atom: Atom {
                        table: Path::from("T"),
                        row_id: Some(Term::Var { index: 0 }),
                        values: vec![],
                    },
                },
            },
        };
        let compiled = compile_law(&law).expect("compile law");
        let mut converter = Converter::new(&compiled);
        converter.table_arities.clear();

        assert_eq!(
            converter.convert(),
            Err(ConvertError::MissingTableArity {
                table: "T".to_string()
            })
        );
    }
}
