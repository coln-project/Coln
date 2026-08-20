// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module converts coln's flattened lowered intermediate representation
//! (FLIR) into a query program expressed in [`Statements`](crate::host::stmt::Stmt),
//! using [`HostExprs`](crate::host::expr::Expr) and [`RelExprs`](crate::relational::expr::RelExpr).

#![allow(unreachable_code)] // Temporary due to todo!() annotations.
use crate::api::schema::{TableRef, TableSchema};
use crate::error::SyntaxError;
use crate::host::Code;
use crate::host::expr::{BinaryExpr, Expr, Literal, LiteralExpr, VarExpr};
use crate::host::operator::Operator;
use crate::host::stmt::{Stmt, VarStmt};
use crate::relational::RelationSchema;
use crate::relational::expr::{
    AntiJoinExpr, EquiJoinExpr, MultiWayEquiJoin, ProjectionExpr, SelectionExpr, SourceExpr,
};
use coln_flir_rs::ir::{
    self, Atom, EntityVariant, Equality, FlatRealm, Path, Prop, RuleEntry, TableEntry, Term,
};
use coln_flir_rs::schema::{BaseTableSchema, CompilerColIdx, StoreEngineCols};
use std::collections::HashMap;

type BaseTableName = TableRef;
type DerivedViewName = TableRef;

/// An identifier that uniquely identifies a table (globally across the store).
#[derive(Eq, PartialEq, Hash, Debug, Clone)]
pub struct RuleName {
    inner: String,
}

impl<T: Into<String>> From<T> for RuleName {
    fn from(value: T) -> Self {
        RuleName {
            inner: value.into(),
        }
    }
}

struct QueryProgramBuilder {
    program: Code,
    base_tables: HashMap<BaseTableName, BaseTableSchema>,
    derived_views: HashMap<DerivedViewName, TableSchema>,
    rules: HashMap<RuleName, RuleMeta>,
}

struct RuleMeta {
    kind: ir::RuleVariant,
    output_schema: TableSchema,
}

impl RuleMeta {
    fn new(kind: ir::RuleVariant, output_schema: TableSchema) -> Self {
        RuleMeta {
            kind,
            output_schema,
        }
    }
}

impl From<&BaseTableSchema> for RelationSchema {
    fn from(value: &BaseTableSchema) -> Self {
        RelationSchema::new(
            value.name().to_string(),
            value.query_cols().iter().map(|col| col.name().to_string()),
            value
                .query_cols()
                .iter()
                // The first two columns are the row id columns and can act as
                // the key for now.
                .take(2)
                .map(|col| col.name().to_string()),
        )
        .expect("Actually infallible")
    }
}

impl QueryProgramBuilder {
    fn new() -> Self {
        Self {
            program: Vec::new(),
            base_tables: HashMap::new(),
            derived_views: HashMap::new(),
            rules: HashMap::new(),
        }
    }
    pub fn from_flat_realm(flat_realm: &FlatRealm) -> Result<Self, SyntaxError> {
        let mut builder = QueryProgramBuilder::new();
        for table in &flat_realm.tables {
            builder.table_declaration(table)?;
        }
        for rule in &flat_realm.rules {
            if rule.rule.consequents.is_empty() {
                // The compiler does not clean up after the lowering and emits
                // useless rules after lowering, so we vacuum-clean here instead.
                continue;
            }
            builder.rule_declaration(rule)?;
        }
        Ok(builder)
    }

    fn table_declaration(&mut self, table_entry: &TableEntry) -> Result<(), SyntaxError> {
        match &table_entry.table.entity_variant {
            EntityVariant::Table => self.base_table(table_entry),
            EntityVariant::View(materialization) => {
                unimplemented!("[Initial models] Materialized views defined through a query");
            }
            EntityVariant::Index { method, columns } => {
                unimplemented!("[Not-yet specified] Indexes")
            }
        }
    }
    fn base_table(&mut self, table_entry: &ir::TableEntry) -> Result<(), SyntaxError> {
        let name = BaseTableName::from(&table_entry.path);
        let table_schema =
            Option::<BaseTableSchema>::from(table_entry).expect("Broken precondition");
        self.base_tables
            .insert(name.clone(), table_schema)
            .ok_or_else(|| SyntaxError::new(format!("Base table {name} defined multiple times")))
            .map(|_old_entry| ())
    }

    fn rule_declaration(&mut self, rule_entry: &RuleEntry) -> Result<(), SyntaxError> {
        let name = rule_entry.path.to_string();
        let Some(rule) = FriendlyRule::from(&rule_entry.rule) else {
            // The rule is filtered out but not an error case.
            return Ok(());
        };
        let stmt = self.rule(name, &rule)?;
        self.program.push(stmt);
        let rule_meta = RuleMeta::new(rule.kind, todo!("Table schema from rule declaration"));
        self.rules
            .insert(RuleName::from(&name), rule_meta)
            .ok_or_else(|| SyntaxError::new(format!("Rule {name} defined multiple times")))?;
        Ok(())
    }
    fn rule(&mut self, name: String, rule: &FriendlyRule) -> Result<Stmt, SyntaxError> {
        let left = self.conjunctive_query(&rule.lhs, &rule.vars)?;
        let right = self.conjunctive_query(&rule.rhs, &rule.vars)?;
        let rule_as_stmt = Stmt::from(VarStmt {
            name,
            initializer: Some(Expr::from(AntiJoinExpr {
                left,
                right,
                on: todo!("Take intersection of vars"),
            })),
        });
        Ok(rule_as_stmt)
    }
    fn conjunctive_query(
        &mut self,
        query: &ConjunctiveQuery,
        vars: &Vec<FriendlyVar>,
    ) -> Result<Expr, SyntaxError> {
        if query.atoms.is_empty() {
            return Err(SyntaxError::new(
                "FLIR emits conjunctive query with no atom",
            ));
        }

        let joined_atoms = Expr::from(MultiWayEquiJoin {
            relations: query
                .atoms
                .iter()
                .map(|atom| self.atom(atom, vars))
                .collect::<Result<Vec<_>, _>>()?,
            // TODO:
            on: vec![],
            attributes: None,
        });

        let with_conditions = query
            .conditions
            .iter()
            .map(|condition| self.selection(condition, vars))
            // // All conditions get compiled into one condition by ANDing them.
            .try_reduce(|acc, conditions| {
                Ok(Expr::from(BinaryExpr {
                    operator: Operator::And,
                    left: acc,
                    right: conditions,
                }))
            })?
            .into_iter()
            // We fold the Option: If there are no conditions at all, we return
            // `joined_atoms` as is and otherwise, we wrap it in a SelectionExpr
            // whose condition embodies all conditions.
            .fold(joined_atoms, |joined_atoms, conditions| {
                Expr::from(SelectionExpr {
                    relation: joined_atoms,
                    condition: conditions,
                })
            });

        Ok(with_conditions)
    }
    /// Generates a condition which possibly expands to two ANDed conditions
    /// due to row ids being flattening to two variables.
    ///
    /// Currently, the compiler only supports equality conditions.
    fn selection(
        &mut self,
        condition: &Equality,
        vars: &Vec<FriendlyVar>,
    ) -> Result<Expr, SyntaxError> {
        let left = self.term(&condition.left, vars)?;
        let right = self.term(&condition.right, vars)?;
        // Things get a bit ugly unfortunately due to the flattening of row ids.
        let conditions: Box<dyn Iterator<Item = (Expr, Expr)>> = match (left.len(), right.len()) {
            (2, 2) => {
                // This case compares two row ids which expand to two variables
                // each and thus we have to create two conditions.
                // The underlying condition has to be true for both the hash
                // _and_ the counter. In code that translates to the diagonal of
                // the terms.
                let diagonal = left.into_iter().zip(right.into_iter());
                Box::new(diagonal)
            }
            _ => {
                // This case deals with comparing:
                // 1. An already flat variable with a literal.
                // But also covers two nonsense cases at the moment:
                // 1. A row id with a literal or an already flat variable.
                // 2. Two literals.
                // In code this boils down to computing all pairs of the terms.
                let cartesian_product = left
                    .into_iter()
                    .flat_map(|left| right.iter().map(move |right| (left.clone(), right.clone())));
                Box::new(cartesian_product)
            }
        };

        Ok(conditions
            .map(|(left, right)| {
                Expr::from(BinaryExpr {
                    operator: Operator::Equal,
                    left,
                    right,
                })
            })
            .reduce(|acc, condition| {
                Expr::from(BinaryExpr {
                    operator: Operator::And,
                    left: acc,
                    right: condition,
                })
            })
            .expect("A FLIR condition must produce at least one condition"))
    }
    fn atom(&mut self, atom: &Atom, vars: &Vec<FriendlyVar>) -> Result<Expr, SyntaxError> {
        let (source, schema): (Expr, &BaseTableSchema) =
            if let Some((source_expr, schema)) = self.base_table_source_expr(&atom.entity) {
                (Expr::from(source_expr), schema)
            } else if let Some(var_expr) = self.derived_view_var_expr(&atom.entity) {
                (
                    Expr::from(var_expr),
                    todo!("Generic schema representation for derived views"),
                )
            } else {
                return Err(SyntaxError::new(format!(
                    "Atom references undeclared entity '{}'",
                    atom.entity
                )));
            };

        let attributes: Vec<(String, Expr)> = if let Some(row_id) = &atom.row_id {
            // TODO: find abstraction for this Term dance.
            match row_id {
                ir::Term::Var { index } => {
                    let var = vars
                        .get(*index as usize)
                        .ok_or_else(|| SyntaxError::new("FLIR var idx out of bounds"))?;
                    match &var.ty {
                        ir::ColType::RowId { path } => {
                            let mut columns =
                                schema.resolve_query_cols(CompilerColIdx::for_row_id());
                            let column_hash = columns.next().expect("No row id hash column");
                            let column_ctr = columns.next().expect("No row id counter column");
                            vec![
                                (
                                    var.name
                                        .clone()
                                        .append(StoreEngineCols::HASH_COL_SUFFIX)
                                        .to_string(),
                                    Expr::from(VarExpr::new(column_hash.name())),
                                ),
                                (
                                    var.name
                                        .clone()
                                        .append(StoreEngineCols::CTR_COL_SUFFIX)
                                        .to_string(),
                                    Expr::from(VarExpr::new(column_ctr.name())),
                                ),
                            ]
                        }
                        ir::ColType::BuiltinTy { builtin_ty } => {
                            return Err(SyntaxError::new(
                                "FLIR wants to assign a row id to a variable of native scalar type",
                            ));
                        }
                    }
                }
                ir::Term::Lit { lit } => {
                    return Err(SyntaxError::new(
                        "FLIR expects row id to be equal to a literal",
                    ));
                }
            }
        } else {
            vec![]
        };

        let (conditions, attributes) = atom.values.iter().try_fold((vec![], attributes), |(mut conditions, mut attributes), value| {
            let mut columns = schema.resolve_query_cols(CompilerColIdx::from(value.column));
            match &value.term {
                ir::Term::Lit { lit } => {
                    let column = columns.next().expect("A literal can only ever be compared to a single column because it cannot store a row id");
                    conditions.push(Expr::from(BinaryExpr {
                        operator: Operator::Equal,
                        left: Expr::from(VarExpr::new(column.name())),
                        right: Expr::from(LiteralExpr::from(Literal::from(lit))),
                    }))
                },
                ir::Term::Var { index } => {
                    let var = vars
                        .get(*index as usize)
                        .ok_or_else(|| SyntaxError::new("FLIR var idx out of bounds"))?;
                    match &var.ty {
                        ir::ColType::BuiltinTy { builtin_ty } => {
                            let column = columns.next().expect("A var of a native scalar type can only ever reference a single value column");
                            attributes.push((var.name.to_string(), Expr::from(
                                VarExpr::new(column.name()),
                            )))
                        },
                        ir::ColType::RowId { path } => {
                            let column_hash = columns.next().expect("No row id hash column");
                            let column_ctr = columns.next().expect("No row id counter column");
                            attributes.push((var.name.clone().append(StoreEngineCols::HASH_COL_SUFFIX).to_string(), Expr::from(
                                VarExpr::new(column_hash.name())
                            )));
                            attributes.push((var.name.clone().append(StoreEngineCols::CTR_COL_SUFFIX).to_string(), Expr::from(
                                VarExpr::new(column_ctr.name())
                            )));
                        },
                    }
                },
            };
            Ok((conditions, attributes))
        })?;

        let with_selection = conditions
            .into_iter()
            .reduce(|acc, condition| {
                Expr::from(BinaryExpr {
                    operator: Operator::And,
                    left: acc,
                    right: condition,
                })
            })
            .into_iter()
            .fold(source, |source, root_condition| {
                Expr::from(SelectionExpr {
                    relation: source,
                    condition: root_condition,
                })
            });

        let with_projection = Expr::from(ProjectionExpr {
            relation: with_selection,
            attributes,
        });

        Ok(with_projection)
    }
    fn term(&mut self, term: &Term, vars: &Vec<FriendlyVar>) -> Result<Vec<Expr>, SyntaxError> {
        match term {
            Term::Lit { lit } => Ok(vec![Expr::from(LiteralExpr::from(Literal::from(lit)))]),
            Term::Var { index } => {
                let var = vars
                    .get(*index as usize)
                    .ok_or_else(|| SyntaxError::new("FLIR var idx out of bounds"))?;
                match &var.ty {
                    ir::ColType::BuiltinTy { builtin_ty: _ } => {
                        Ok(vec![Expr::from(VarExpr::new(var.name.to_string()))])
                    }
                    ir::ColType::RowId { path: _ } => Ok(vec![
                        Expr::from(VarExpr::new(
                            var.name.clone().append(StoreEngineCols::HASH_COL_SUFFIX),
                        )),
                        Expr::from(VarExpr::new(
                            var.name.clone().append(StoreEngineCols::CTR_COL_SUFFIX),
                        )),
                    ]),
                }
            }
        }
    }
    /// If the entity referenced by `Path` is part of the extensional database
    /// (EDB) and present in the base tables, the function returns a
    /// [`SourceExpr`] referencing that entity. Otherwise, [`None`] is returned.
    fn base_table_source_expr(&mut self, name: &Path) -> Option<(SourceExpr, &BaseTableSchema)> {
        self.base_tables
            .get(&BaseTableName::from(name))
            .map(|base_table_schema| {
                (
                    SourceExpr {
                        schema: RelationSchema::from(base_table_schema),
                    },
                    base_table_schema,
                )
            })
    }
    /// If the entity referenced by `Path` is part of the intensional database
    /// (IDB) and present in the derived views, the function returns a
    /// [`VarExpr`] referencing that entity. Due to coln-compiler declaring
    /// tables and views prior to the rules, said entity must be known at this
    /// point. Otherwise, [`None`] is returned.
    fn derived_view_var_expr(&mut self, name: &Path) -> Option<VarExpr> {
        self.derived_views
            .get(&DerivedViewName::from(name))
            .map(|_derived_view_schema| VarExpr::new(name.to_string()))
    }
}

/// Just like [`ir::Rule`] but friendlier because:
///
/// 1. Meaningless rules with an empty [consequent](Rule::consequents) are
///    skipped and chased rules panic at the moment due to open questions.
/// 2. It zips the [`Rule::var_names`] and the [`Rule::var_types`] into one
///    array of [`FriendlyVar`]s.
/// 3. It converts [`Rule::antecedents`] and [`Rule::consequents`] into a
///    [`ConjunctiveQuery`], each.
struct FriendlyRule {
    kind: ir::RuleVariant,
    vars: Vec<FriendlyVar>,
    lhs: ConjunctiveQuery,
    rhs: ConjunctiveQuery,
}

impl FriendlyRule {
    fn from(rule: &ir::Rule) -> Option<FriendlyRule> {
        if rule.consequents.is_empty() {
            return None;
        }
        if matches!(rule.rule_variant, ir::RuleVariant::Chased) {
            unimplemented!(
                "[Unclear] Chased rules produce a materialized view; how are they different from a materialized view defined in the table/entities section?"
            );
        }
        assert!(
            rule.var_names.len() == rule.var_types.len(),
            "var_names and var_types arrays do not size match"
        );
        let vars = rule
            .var_names
            .iter()
            .zip(rule.var_types.iter())
            .map(|(path, col_type)| FriendlyVar {
                name: path.clone(),
                ty: col_type.clone(),
            })
            .collect();
        let lhs = ConjunctiveQuery::from(&rule.antecedents);
        let rhs = ConjunctiveQuery::from(&rule.consequents);
        Some(FriendlyRule {
            kind: rule.rule_variant,
            vars,
            lhs,
            rhs,
        })
    }
}

/// Prepares either a [left-hand side](Rule::antecedents) or a
/// [right-hand side](Rule::consequents) of a [`Rule`] for inclusion in an
/// antijoin by partitioning a `Vec<Prop>` into atoms and conditions. This is
/// useful because applying all atoms first, guarantees that every variable a
/// condition may refer to is in scope already.
struct ConjunctiveQuery {
    atoms: Vec<ir::Atom>,
    // Currently, only equality conditions are part of the FLIR.
    conditions: Vec<ir::Equality>,
}

impl ConjunctiveQuery {
    fn from(props: &Vec<Prop>) -> Self {
        let (atoms, conditions) =
            props
                .iter()
                .fold((vec![], vec![]), |(mut atoms, mut conditions), prop| {
                    match prop {
                        ir::Prop::Atom { atom } => atoms.push(atom.clone()),
                        ir::Prop::Eq { equality } => conditions.push(equality.clone()),
                    }
                    (atoms, conditions)
                });
        Self { atoms, conditions }
    }
}

/// All information from [`ir::Rule::var_names`] and [`ir::Rule::var_types`] but
/// _zipped_.
struct FriendlyVar {
    name: ir::Path,
    ty: ir::ColType, // either a row id or a builtin type
}

pub trait TryReduceResultExt<T, E>: Iterator<Item = Result<T, E>> {
    /// Reduces an iterator yielding `Result<T, E>`, short-circuiting if an `Err`
    /// is yielded or if the reduction closure returns an `Err`.
    fn try_reduce<F>(mut self, mut f: F) -> Result<Option<T>, E>
    where
        Self: Sized,
        F: FnMut(T, T) -> Result<T, E>,
    {
        let first = match self.next() {
            Some(Ok(v)) => v,
            Some(Err(e)) => return Err(e),
            None => return Ok(None),
        };

        self.try_fold(first, |acc, item| f(acc, item?)).map(Some)
    }
}

impl<I, T, E> TryReduceResultExt<T, E> for I where I: Iterator<Item = Result<T, E>> {}
