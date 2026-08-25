// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module converts coln's flattened lowered intermediate representation
//! (FLIR) into a query program expressed in [`Statements`](crate::host::stmt::Stmt),
//! using [`HostExprs`](crate::host::expr::Expr) and [`RelExprs`](crate::relational::expr::RelExpr).

use crate::error::SyntaxError;
use crate::host::Code;
use crate::host::expr::{BinaryExpr, Expr, Literal, LiteralExpr, VarExpr};
use crate::host::operator::Operator;
use crate::host::stmt::{Stmt, VarStmt};
use crate::program::QueryProgram;
use crate::relational::catalog::Catalog;
use crate::relational::expr::{
    AntiJoinExpr, JoinVariable, MultiWayEquiJoinExpr, ProjectionExpr, RelationIdx, SelectionExpr,
    SourceExpr, SourceId,
};
use crate::relational::schema::{Column, TableRef, TableSchema};
use crate::scalarial::ScalarType;
use coln_flir_rs::ir::{
    self, Atom, EntityVariant, Equality, FlatRealm, Path, Prop, RuleEntry, TableEntry, Term,
};
use coln_flir_rs::schema::{BaseTableSchema, CompilerColIdx, QueryEngineCol, StoreEngineCols};
use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};

type BaseTableName = TableRef;
type DerivedViewName = TableRef;

/// coln's FLIR frontend's [`QueryProgram`]: what a [`FlatRealm`] lowers to.
///
/// The [`Catalog`] half is served straight out of [`base_tables`](Self::base_tables),
/// which stores FLIR's own richer [`BaseTableSchema`] — column indices, store-engine
/// columns and all — rather than a second copy of what a query plan needs. That
/// is exactly the freedom [`Catalog::source_schema`]'s [`Cow`] return buys.
pub struct FlirProgram {
    /// The (raw, that is, unresolved, unoptimized) statements themselves.
    code: Code,
    /// The declared base tables. Doubles as this program's [`Catalog`]: every
    /// [`SourceExpr`] the lowering mints names one of these.
    base_tables: HashMap<BaseTableName, BaseTableSchema>,
    /// The relations the program itself defines, that is, one per declared rule.
    ///
    /// This doubles as the set of derived views an [`Atom`] may reference, so
    /// that what [`rule_declaration`](Self::rule_declaration) writes is exactly
    /// what [`derived_view_var_expr`](Self::derived_view_var_expr) reads.
    derived_views: HashMap<DerivedViewName, RuleMeta>,
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

/// Projects FLIR's per-engine schema down to the one thing the layers below
/// share: the query engine's columns, and the table's key(s) restated over
/// them.
///
/// Both halves change coordinates on the way. The columns are the *query*
/// engine's view, where a row id has already flattened into a hash and a
/// counter column; FLIR states its keys as indices into the *compiler's*
/// view, so each component of a key travels through
/// [`resolve_query_col_range`](BaseTableSchema::resolve_query_col_range) to
/// become the one or two positions it occupies here.
///
/// The implicit row id leads the list of keys: it is the only key a base table
/// is guaranteed to have and to be unique on, so a backend that can index by
/// just one key (DBSP) picks it by taking the first.
impl From<&BaseTableSchema> for TableSchema {
    fn from(value: &BaseTableSchema) -> Self {
        let columns = value
            .query_cols()
            .iter()
            .map(|col| Column::new(col.name(), *col.ty()))
            .collect();
        let row_id_key = value
            .resolve_query_col_range(CompilerColIdx::for_row_id())
            .collect();
        let declared_keys = value
            .primary_keys()
            .iter()
            // The compiler reports a table without a declared primary key as one
            // empty key rather than no key at all.
            .filter(|key| !key.is_empty())
            .map(|key| {
                key.iter()
                    .flat_map(|idx| value.resolve_query_col_range(*idx))
                    .collect()
            });
        TableSchema::new(
            TableRef::from(value.name()),
            columns,
            std::iter::once(row_id_key).chain(declared_keys).collect(),
        )
    }
}

impl FlirProgram {
    fn empty() -> Self {
        Self {
            code: Code::default(),
            base_tables: HashMap::new(),
            derived_views: HashMap::new(),
        }
    }
    pub fn from_flat_realm(flat_realm: &FlatRealm) -> Result<Self, SyntaxError> {
        let mut builder = FlirProgram::empty();
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
        if self
            .base_tables
            .insert(name.clone(), table_schema)
            .is_some()
        {
            return Err(SyntaxError::new(format!(
                "Base table {name} defined multiple times"
            )));
        }
        Ok(())
    }

    fn rule_declaration(&mut self, rule_entry: &RuleEntry) -> Result<(), SyntaxError> {
        let name = DerivedViewName::from(&rule_entry.path);
        let Some(rule) = FriendlyRule::from(&rule_entry.rule) else {
            // The rule is filtered out but not an error case.
            return Ok(());
        };
        let (stmt, output_bindings) = self.rule(name.to_string(), &rule)?;
        self.code.push(stmt);
        let rule_meta = RuleMeta::new(rule.kind, rule_output_schema(&name, &output_bindings));
        // See `base_table` on the direction of this check.
        if self.derived_views.insert(name.clone(), rule_meta).is_some() {
            return Err(SyntaxError::new(format!(
                "Rule {name} defined multiple times"
            )));
        }
        Ok(())
    }
    /// Lowers one rule into the statement that binds its name, and reports the
    /// [`Binding`]s of the relation that statement evaluates to, so the caller
    /// can describe the rule's output schema.
    fn rule(
        &mut self,
        name: String,
        rule: &FriendlyRule,
    ) -> Result<(Stmt, Vec<Binding>), SyntaxError> {
        let (left, left_bindings) = self.conjunctive_query(&rule.lhs, &rule.vars)?;
        let (right, right_bindings) = self.conjunctive_query(&rule.rhs, &rule.vars)?;
        let rule_as_stmt = Stmt::from(VarStmt {
            name,
            initializer: Some(Expr::from(AntiJoinExpr {
                left,
                right,
                on: antijoin_key(&left_bindings, &right_bindings),
            })),
        });
        // An antijoin carries the left relation's tuple through unchanged, so
        // the rule's output is shaped by its antecedents.
        Ok((rule_as_stmt, left_bindings))
    }
    /// Lowers one side of a rule into a relational expression, and reports which
    /// variable parts that expression binds so the enclosing [`AntiJoinExpr`]
    /// can work out what to compare on.
    fn conjunctive_query(
        &mut self,
        query: &ConjunctiveQuery,
        vars: &[FriendlyVar],
    ) -> Result<(Expr, Vec<Binding>), SyntaxError> {
        if query.atoms.is_empty() {
            return Err(SyntaxError::new(
                "FLIR emits conjunctive query with no atom",
            ));
        }

        let plans = query
            .atoms
            .iter()
            .map(|atom| self.atom(atom, vars))
            .collect::<Result<Vec<_>, _>>()?;

        // A part bound by several atoms is a single binding of the conjunctive
        // query as a whole, because the join keeps one active copy of it. We
        // keep the first, matching the join's left-to-right shadowing.
        let mut bindings: BTreeMap<(ir::VarIdx, VarPart), Binding> = BTreeMap::new();
        for plan in &plans {
            for binding in &plan.bindings {
                bindings
                    .entry((binding.var, binding.part))
                    .or_insert_with(|| binding.clone());
            }
        }
        let bindings = bindings.into_values().collect();

        let on = join_variables(&plans);
        let mut relations: Vec<Expr> = plans.into_iter().map(|plan| plan.relation).collect();
        let joined_atoms = if relations.len() == 1 {
            // A single atom has nothing to join against, and the join operators
            // require at least two relations.
            relations.pop().expect("Length checked")
        } else {
            Expr::from(MultiWayEquiJoinExpr::new(relations, on, None)?)
        };

        let with_conditions = query
            .conditions
            .iter()
            .map(|condition| self.selection(condition, vars))
            // All conditions get compiled into one condition by ANDing them.
            .try_reduce(|left, right| {
                Expr::from(BinaryExpr {
                    operator: Operator::And,
                    left,
                    right,
                })
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

        Ok((with_conditions, bindings))
    }
    /// Generates a condition which possibly expands to two ANDed conditions
    /// due to row ids being flattening to two variables.
    ///
    /// Currently, the compiler only supports equality conditions.
    fn selection(
        &mut self,
        condition: &Equality,
        vars: &[FriendlyVar],
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
                let diagonal = left.into_iter().zip(right);
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
    // Scoped to this function because of the derived-view `todo!()` below;
    // the rest of the module is checked for unreachable code again.
    #[allow(unreachable_code)]
    fn atom(&mut self, atom: &Atom, vars: &[FriendlyVar]) -> Result<AtomPlan, SyntaxError> {
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

        let mut binder = AtomBinder::default();

        // The row id, if this atom brings it into scope.
        if let Some(row_id) = &atom.row_id {
            match row_id {
                ir::Term::Var { index } => {
                    let var = friendly_var(vars, *index)?;
                    if !var.is_row_id() {
                        return Err(SyntaxError::new(
                            "FLIR wants to assign a row id to a variable of native scalar type",
                        ));
                    }
                    binder.bind(
                        *index,
                        var,
                        schema.resolve_query_cols(CompilerColIdx::for_row_id()),
                    )?;
                }
                ir::Term::Lit { lit: _ } => {
                    // Matching [`ir::Atom::row_id`]'s own note: a literal row id
                    // is not something we can express.
                    return Err(SyntaxError::new(
                        "FLIR equates a row id with a literal, which is not supported",
                    ));
                }
            }
        }

        // The value columns this atom constrains or brings into scope.
        for value in &atom.values {
            let mut columns = schema.resolve_query_cols(CompilerColIdx::from(value.column));
            match &value.term {
                ir::Term::Lit { lit } => {
                    let column = columns.next().ok_or_else(|| {
                        SyntaxError::new("FLIR compares a literal against a column that does not resolve to any query column")
                    })?;
                    binder.conditions.push(Expr::from(BinaryExpr {
                        operator: Operator::Equal,
                        left: Expr::from(VarExpr::new(column.name())),
                        right: Expr::from(LiteralExpr::from(Literal::from(lit))),
                    }));
                }
                ir::Term::Var { index } => {
                    binder.bind(*index, friendly_var(vars, *index)?, columns)?;
                }
            }
        }

        let with_selection = binder
            .conditions
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

        let relation = Expr::from(ProjectionExpr {
            relation: with_selection,
            attributes: binder.attributes,
        });

        Ok(AtomPlan {
            relation,
            bindings: binder.bindings,
        })
    }
    fn term(&mut self, term: &Term, vars: &[FriendlyVar]) -> Result<Vec<Expr>, SyntaxError> {
        match term {
            Term::Lit { lit } => Ok(vec![Expr::from(LiteralExpr::from(Literal::from(lit)))]),
            Term::Var { index } => Ok(friendly_var(vars, *index)?
                .parts()
                .map(|(_part, name)| Expr::from(VarExpr::new(name)))
                .collect()),
        }
    }
    /// If the entity referenced by `Path` is part of the extensional database
    /// (EDB) and present in the base tables, the function returns a
    /// [`SourceExpr`] referencing that entity. Otherwise, [`None`] is returned.
    fn base_table_source_expr(&mut self, name: &Path) -> Option<(SourceExpr, &BaseTableSchema)> {
        self.base_tables
            .get(&BaseTableName::from(name))
            .map(|base_table_schema| {
                // The leaf names the table; `Catalog::source_schema` below is
                // what turns that name back into a schema, and both sides go
                // through `BaseTableSchema::name` so they cannot disagree.
                (
                    SourceExpr::new(base_table_schema.name().to_string()),
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

impl Catalog for FlirProgram {
    /// Projects FLIR's [`BaseTableSchema`] down to the [`TableSchema`] a plan
    /// needs, on demand. [`Cow::Owned`] rather than a borrow precisely so that
    /// the richer schema stays the only stored copy.
    ///
    /// Only base tables answer here: a rule's output is bound to a host variable
    /// and referenced by [`VarExpr`], never by a [`SourceExpr`], so
    /// [`derived_views`](Self::derived_views) is no part of the catalog.
    fn source_schema(&self, id: &SourceId) -> Option<Cow<'_, TableSchema>> {
        self.base_tables
            .get(&BaseTableName::from(id))
            .map(|base_table_schema| Cow::Owned(TableSchema::from(base_table_schema)))
    }
}

impl QueryProgram for FlirProgram {
    fn code(&self) -> &Code {
        &self.code
    }

    fn take_code(&mut self) -> Code {
        std::mem::take(&mut self.code)
    }
}

/// Just like [`ir::Rule`] but friendlier because:
///
/// 1. Meaningless rules with an empty [consequent](ir::Rule::consequents) are
///    skipped and chased rules panic at the moment due to open questions.
/// 2. It zips the [`ir::Rule::var_names`] and the [`ir::Rule::var_types`] into one
///    array of [`FriendlyVar`]s.
/// 3. It converts [`ir::Rule::antecedents`] and [`ir::Rule::consequents`] into a
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

/// Prepares either a [left-hand side](ir::Rule::antecedents) or a
/// [right-hand side](ir::Rule::consequents) of a [`ir::Rule`] for inclusion in an
/// antijoin by partitioning a `Vec<Prop>` into atoms and conditions. This is
/// useful because applying all atoms first, guarantees that every variable a
/// condition may refer to is in scope already.
struct ConjunctiveQuery {
    atoms: Vec<ir::Atom>,
    // Currently, only equality conditions are part of the FLIR.
    conditions: Vec<ir::Equality>,
}

impl ConjunctiveQuery {
    fn from(props: &[Prop]) -> Self {
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

impl FriendlyVar {
    fn is_row_id(&self) -> bool {
        matches!(self.ty, ir::ColType::RowId { path: _ })
    }
    /// The attribute name(s) this variable expands to in an atom's projected
    /// schema: one for a builtin scalar type, two for a row id, which flattens
    /// into a commit hash and a counter column.
    ///
    /// This is the single place that flattening happens, and its order matches
    /// the order
    /// [`resolve_query_cols`](BaseTableSchema::resolve_query_cols) yields the
    /// corresponding columns in, so the two can be zipped.
    fn parts(&self) -> impl Iterator<Item = (VarPart, String)> {
        match &self.ty {
            ir::ColType::BuiltinTy { builtin_ty: _ } => {
                vec![(VarPart::Scalar, self.name.to_string())]
            }
            ir::ColType::RowId { path: _ } => vec![
                (
                    VarPart::RowIdHash,
                    self.name
                        .clone()
                        .append(StoreEngineCols::HASH_COL_SUFFIX)
                        .to_string(),
                ),
                (
                    VarPart::RowIdCtr,
                    self.name
                        .clone()
                        .append(StoreEngineCols::CTR_COL_SUFFIX)
                        .to_string(),
                ),
            ],
        }
        .into_iter()
    }
}

/// Which of the attributes a FLIR variable expands to, see
/// [`FriendlyVar::parts`]. The derived ordering keeps a row id's two halves
/// adjacent and in flattening order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum VarPart {
    /// The variable in its entirety. It is of a builtin scalar type.
    Scalar,
    /// The commit-hash half of a row id.
    RowIdHash,
    /// The counter half of a row id.
    RowIdCtr,
}

/// One attribute an atom's projection exposes, tagged with the FLIR variable it
/// originates from.
///
/// The join condition of a conjunctive query is derived by grouping these on
/// [`var`](Self::var) and [`part`](Self::part) rather than on
/// [`name`](Self::name): the variable index is exact and free, whereas grouping
/// by name would additionally assume that rendering [`ir::Path`]s into strings
/// is injective, and would have to tell a row id's two halves apart by parsing
/// their suffixes back off.
#[derive(Clone, Debug)]
struct Binding {
    var: ir::VarIdx,
    part: VarPart,
    /// The attribute's name in the atom's projected schema.
    name: String,
    /// The type of the query column this part is bound to. Taken from the
    /// column rather than from the FLIR variable, because that is where a row
    /// id's halves have already been resolved to their query-engine types.
    scalar_type: ScalarType,
}

/// The relational plan for one FLIR [`Atom`], together with the [`Binding`]s
/// its projection exposes.
///
/// Reporting the bindings is what lets the enclosing conjunctive query derive
/// its join variables without re-deriving them from the projection it just
/// built.
struct AtomPlan {
    /// Essentially, a `Projection(Selection(atom's source relation))`.
    relation: Expr,
    /// The [`Binding`]s of the plan.
    bindings: Vec<Binding>,
}

/// Accumulates what one atom contributes while its row id and value terms are
/// walked.
#[derive(Default)]
struct AtomBinder {
    /// Conditions local to this atom: literal comparisons, plus the equalities
    /// that a variable repeated within this one atom gives rise to, that is,
    /// `atom(x, x)`.
    conditions: Vec<Expr>,
    /// The atom's projection, mapping each bound variable part onto the query
    /// column carrying it.
    attributes: Vec<(String, Expr)>,
    bindings: Vec<Binding>,
    /// The query column each variable part was *first* bound to in this atom, so
    /// a repeated occurrence can be turned into an equality against it.
    bound: HashMap<(ir::VarIdx, VarPart), String>,
}

impl AtomBinder {
    /// Binds `var`'s parts to `columns`, which must resolve to one query column
    /// per part.
    fn bind<'a>(
        &mut self,
        index: ir::VarIdx,
        var: &FriendlyVar,
        columns: impl Iterator<Item = &'a QueryEngineCol>,
    ) -> Result<(), SyntaxError> {
        let parts: Vec<(VarPart, String)> = var.parts().collect();
        let columns: Vec<&QueryEngineCol> = columns.collect();
        if parts.len() != columns.len() {
            return Err(SyntaxError::new(format!(
                "FLIR binds variable '{}', which flattens into {} column(s), to a \
                 column resolving to {} query column(s)",
                var.name,
                parts.len(),
                columns.len()
            )));
        }
        for ((part, name), column) in parts.into_iter().zip(columns) {
            let scalar_type = ScalarType::from(*column.ty());
            let column = column.name().to_string();
            match self.bound.entry((index, part)) {
                Entry::Vacant(slot) => {
                    self.attributes
                        .push((name.clone(), Expr::from(VarExpr::new(column.clone()))));
                    self.bindings.push(Binding {
                        var: index,
                        part,
                        name,
                        scalar_type,
                    });
                    slot.insert(column);
                }
                Entry::Occupied(first) => {
                    // The variable is repeated within this single atom, as in
                    // `R(x, x)`. That is a local equality condition on this one
                    // relation rather than a join condition, and the projection
                    // has to expose the attribute exactly once — two attributes
                    // of the same name would collide in the projected schema.
                    self.conditions.push(Expr::from(BinaryExpr {
                        operator: Operator::Equal,
                        left: Expr::from(VarExpr::new(first.get().clone())),
                        right: Expr::from(VarExpr::new(column)),
                    }));
                }
            }
        }
        Ok(())
    }
}

/// The schema of the relation a rule evaluates to.
///
/// Its columns are the parts the rule's output binds, in the
/// `(VarIdx, VarPart)` order [`FlirProgram::conjunctive_query`] reports
/// them in, and their types come from the query columns those parts resolve to
/// rather than from the FLIR variables — a row id's two halves reach the query
/// engine as plain unsigned integers, which the variable's [`ir::ColType`] does
/// not say.
fn rule_output_schema(name: &TableRef, bindings: &[Binding]) -> TableSchema {
    TableSchema::new(
        name.clone(),
        bindings
            .iter()
            .map(|binding| Column::new(binding.name.clone(), binding.scalar_type))
            .collect(),
        // A rule declares no key of its own, and nothing consumes the primary
        // keys of a derived relation yet. The row id parts it binds would be the
        // candidate once something does.
        vec![],
    )
}

fn friendly_var(vars: &[FriendlyVar], index: ir::VarIdx) -> Result<&FriendlyVar, SyntaxError> {
    vars.get(index as usize)
        .ok_or_else(|| SyntaxError::new(format!("FLIR var idx {index} out of bounds")))
}

/// Derives the join condition of a conjunctive query: one [`JoinVariable`] per
/// variable part that more than one atom binds.
///
/// A part bound by a single atom is dropped. It is not an equality class, so it
/// is not part of a join condition — it reaches the output through its atom's
/// schema, which is also how it stays available to an enclosing antijoin.
///
/// Grouping runs through a [`BTreeMap`] keyed on `(VarIdx, VarPart)`, so the
/// resulting order follows the FLIR variable indices instead of a hash order.
/// Plans have to be reproducible for a given input.
fn join_variables(plans: &[AtomPlan]) -> Vec<JoinVariable> {
    let mut occurrences: BTreeMap<(ir::VarIdx, VarPart), Vec<(RelationIdx, String)>> =
        BTreeMap::new();
    for (relation, plan) in plans.iter().enumerate() {
        for binding in &plan.bindings {
            occurrences
                .entry((binding.var, binding.part))
                .or_default()
                .push((relation, binding.name.clone()));
        }
    }
    occurrences
        .into_values()
        .filter(|occurrences| occurrences.len() > 1)
        .map(|occurrences| JoinVariable {
            // Every atom projects a given part onto the same name, so the first
            // occurrence's name is the shared output name — and it is the copy
            // the join keeps active, since shadowing favours the earlier
            // relation.
            name: occurrences[0].1.clone(),
            occurrences: occurrences
                .into_iter()
                .map(|(relation, name)| (relation, Expr::from(VarExpr::new(name))))
                .collect(),
        })
        .collect()
}

/// The key an [`AntiJoinExpr`] between the two sides of a rule compares on:
/// every variable part that both sides bind.
///
/// Note that a part occurring only once *within* a side belongs here all the
/// same. It is not a join variable of that side's conjunctive query, but it is
/// bound by that side's schema, and the antijoin does have to compare on it.
fn antijoin_key(left: &[Binding], right: &[Binding]) -> Vec<(Expr, Expr)> {
    let right: BTreeMap<(ir::VarIdx, VarPart), &str> = right
        .iter()
        .map(|binding| ((binding.var, binding.part), binding.name.as_str()))
        .collect();
    left.iter()
        .filter_map(|binding| {
            right
                .get(&(binding.var, binding.part))
                .map(|counterpart| (binding.name.as_str(), *counterpart))
        })
        .map(|(left, right)| {
            (
                Expr::from(VarExpr::new(left)),
                Expr::from(VarExpr::new(right)),
            )
        })
        .collect()
}

pub trait TryReduce<T, E>: Iterator<Item = Result<T, E>> {
    /// Reduces to a single item, short-circuiting on the first [`Err`].
    ///
    /// Unlike collecting into a `Vec` first, nothing is allocated, and unlike
    /// [`Iterator::reduce`] the items may fail. Note that `f` itself is
    /// infallible: the fallibility belongs to the items, not to the step that
    /// combines two of them.
    fn try_reduce(mut self, mut f: impl FnMut(T, T) -> T) -> Result<Option<T>, E>
    where
        Self: Sized,
    {
        let Some(first) = self.next().transpose()? else {
            return Ok(None);
        };
        self.try_fold(first, |acc, item| Ok(f(acc, item?)))
            .map(Some)
    }
}

impl<I, T, E> TryReduce<T, E> for I where I: Iterator<Item = Result<T, E>> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relational::expr::RelExpr;

    /// A builder with one base table `t` whose columns are given as
    /// `(name, type)` pairs, so [`FlirProgram::atom`] can be driven
    /// directly.
    fn builder_with_table(columns: Vec<(&str, ir::ColType)>) -> FlirProgram {
        let mut builder = FlirProgram::empty();
        builder
            .table_declaration(&table_entry(columns))
            .expect("A single base table declaration must succeed");
        builder
    }

    fn table_entry(columns: Vec<(&str, ir::ColType)>) -> ir::TableEntry {
        ir::TableEntry {
            path: ir::Path::from("t"),
            table: ir::Schema {
                entity_variant: ir::EntityVariant::Table,
                columns: columns
                    .into_iter()
                    .map(|(name, col_type)| ir::ColumnEntry {
                        path: ir::Path::from(name),
                        col_type,
                    })
                    .collect(),
                primary_key: None,
            },
        }
    }

    fn builtin() -> ir::ColType {
        ir::ColType::BuiltinTy {
            builtin_ty: ir::BuiltinTy::BuiltinInt,
        }
    }

    fn atom_over_t(row_id: Option<ir::Term>, values: Vec<(ir::ColumnIdx, ir::Term)>) -> ir::Atom {
        ir::Atom {
            entity: ir::Path::from("t"),
            row_id,
            values: values
                .into_iter()
                .map(|(column, term)| ir::ValueEntry { column, term })
                .collect(),
        }
    }

    fn var_term(index: ir::VarIdx) -> ir::Term {
        ir::Term::Var { index }
    }

    /// Destructures the `Projection(Selection?(Source))` shape an atom lowers to.
    fn assert_projection(expr: &Expr) -> &ProjectionExpr {
        match expr {
            Expr::Relational(RelExpr::Projection(projection)) => projection,
            other => panic!("Expected a relational projection expression, got {other:?}"),
        }
    }

    /// The selection an atom's local conditions produce, if it has any.
    fn maybe_assert_selection(expr: &Expr) -> Option<&SelectionExpr> {
        match expr {
            Expr::Relational(RelExpr::Selection(selection)) => Some(selection),
            _ => None,
        }
    }

    fn attribute_names(projection: &ProjectionExpr) -> Vec<&str> {
        projection
            .attributes
            .iter()
            .map(|(name, _)| name.as_str())
            .collect()
    }

    #[test]
    fn declaring_the_same_base_table_twice_is_an_error() {
        // `HashMap::insert` returns the previous value, so the check's direction
        // matters: the first declaration must pass and the second must not.
        let mut builder = FlirProgram::empty();
        let entry = table_entry(vec![("a", builtin())]);
        builder
            .table_declaration(&entry)
            .expect("The first declaration of a base table must succeed");
        assert!(
            builder.table_declaration(&entry).is_err(),
            "A second declaration of the same base table must be rejected"
        );
    }

    #[test]
    fn an_atom_projects_each_bound_variable_onto_its_column() {
        let mut builder = builder_with_table(vec![("a", builtin()), ("b", builtin())]);
        let vars = vec![scalar_var("x"), scalar_var("y")];
        let plan = builder
            .atom(
                &atom_over_t(None, vec![(0, var_term(0)), (1, var_term(1))]),
                &vars,
            )
            .expect("A well-formed atom lowers");

        assert_eq!(
            attribute_names(assert_projection(&plan.relation)),
            vec!["x", "y"]
        );
        assert_eq!(plan.bindings.len(), 2);
        // No local conditions, so no selection between projection and source.
        assert!(maybe_assert_selection(&assert_projection(&plan.relation).relation).is_none());
    }

    #[test]
    fn a_literal_becomes_a_local_condition_rather_than_a_binding() {
        let mut builder = builder_with_table(vec![("a", builtin())]);
        let plan = builder
            .atom(&atom_over_t(None, vec![(0, lit_term(42))]), &[])
            .expect("An atom comparing a column to a literal lowers");

        assert!(plan.bindings.is_empty());
        assert!(attribute_names(assert_projection(&plan.relation)).is_empty());
        assert!(
            maybe_assert_selection(&assert_projection(&plan.relation).relation).is_some(),
            "The literal must become a selection beneath the projection"
        );
    }

    #[test]
    fn a_variable_repeated_within_one_atom_is_bound_once_and_equated() {
        // `t(x, x)` must not project two attributes called `x` — they would
        // collide in the projected schema. The repetition is a local equality
        // condition on this one relation instead, which is also what keeps the
        // join's relation indices distinct per variable.
        let mut builder = builder_with_table(vec![("a", builtin()), ("b", builtin())]);
        let vars = vec![scalar_var("x")];
        let plan = builder
            .atom(
                &atom_over_t(None, vec![(0, var_term(0)), (1, var_term(0))]),
                &vars,
            )
            .expect("A repeated variable lowers");

        assert_eq!(
            attribute_names(assert_projection(&plan.relation)),
            vec!["x"]
        );
        assert_eq!(plan.bindings.len(), 1);

        let selection = maybe_assert_selection(&assert_projection(&plan.relation).relation)
            .expect("The repetition must produce a selection");
        match &selection.condition {
            Expr::Binary(binary) => {
                assert_eq!(binary.operator, Operator::Equal);
                assert_ne!(
                    binary.left, binary.right,
                    "The equality must compare the two distinct columns"
                );
            }
            other => panic!("Expected an equality condition, got {other:?}"),
        }
    }

    #[test]
    fn a_row_id_variable_binds_both_of_its_halves() {
        let mut builder = builder_with_table(vec![(
            "a",
            ir::ColType::RowId {
                path: ir::Path::from("other"),
            },
        )]);
        let vars = vec![row_id_var("x")];
        let plan = builder
            .atom(&atom_over_t(None, vec![(0, var_term(0))]), &vars)
            .expect("A row id valued column lowers");

        assert_eq!(plan.bindings.len(), 2);
        assert_eq!(attribute_names(assert_projection(&plan.relation)).len(), 2);
        assert_eq!(
            plan.bindings
                .iter()
                .map(|binding| binding.part)
                .collect::<Vec<_>>(),
            vec![VarPart::RowIdHash, VarPart::RowIdCtr]
        );
    }

    #[test]
    fn binding_a_row_id_to_a_scalar_variable_is_an_error() {
        let mut builder = builder_with_table(vec![("a", builtin())]);
        let vars = vec![scalar_var("x")];
        assert!(
            builder
                .atom(&atom_over_t(Some(var_term(0)), vec![]), &vars)
                .is_err()
        );
    }

    /// A rule `antecedents => consequents` over the given variables.
    fn rule_entry(
        name: &str,
        vars: Vec<FriendlyVar>,
        antecedents: Vec<ir::Atom>,
        consequents: Vec<ir::Atom>,
    ) -> ir::RuleEntry {
        let atoms = |atoms: Vec<ir::Atom>| {
            atoms
                .into_iter()
                .map(|atom| ir::Prop::Atom { atom })
                .collect()
        };
        ir::RuleEntry {
            path: ir::Path::from(name),
            rule: ir::Rule {
                rule_variant: ir::RuleVariant::Enforced,
                var_names: vars.iter().map(|var| var.name.clone()).collect(),
                var_types: vars.iter().map(|var| var.ty.clone()).collect(),
                antecedents: atoms(antecedents),
                consequents: atoms(consequents),
            },
        }
    }

    #[test]
    fn a_flat_realm_lowers_into_a_program() {
        let realm = FlatRealm {
            tables: vec![table_entry(vec![("a", builtin()), ("b", builtin())])],
            rules: vec![rule_entry(
                "r",
                vec![scalar_var("x"), scalar_var("y")],
                // t(x, y) and t(x, _) share `x`, so the body is a real join.
                vec![
                    atom_over_t(None, vec![(0, var_term(0)), (1, var_term(1))]),
                    atom_over_t(None, vec![(0, var_term(0))]),
                ],
                vec![atom_over_t(None, vec![(0, var_term(0))])],
            )],
        };

        let builder = FlirProgram::from_flat_realm(&realm).expect("The realm lowers");

        assert_eq!(builder.code().len(), 1, "One rule is one statement");
        let schema = &builder
            .derived_views
            .get(&TableRef::from(&ir::Path::from("r")))
            .expect("The rule must be registered under its own name")
            .output_schema;
        // The antecedents bind `x` and `y`, so both are output columns, with the
        // types of the query columns they resolve to.
        assert_eq!(
            schema
                .columns()
                .iter()
                .map(|column| (column.name(), column.scalar_type()))
                .collect::<Vec<_>>(),
            vec![("x", ScalarType::Iint), ("y", ScalarType::Iint)]
        );
    }

    #[test]
    fn a_lowered_program_passes_the_resolver() {
        // Whatever the lowering emits has to be a well-formed program: every
        // variable resolves, and every relational operator's invariants hold.
        // This is what actually reaches `MultiWayEquiJoinExpr::validate`.
        //
        // It stops short of `Pipeline::runtime`, which would go on to build the
        // DBSP circuit and hit the backend's `unimplemented!` for multi way
        // joins — that needs the fold-into-binary-joins pass.
        let realm = FlatRealm {
            tables: vec![table_entry(vec![("a", builtin()), ("b", builtin())])],
            rules: vec![rule_entry(
                "r",
                vec![scalar_var("x"), scalar_var("y")],
                vec![
                    atom_over_t(None, vec![(0, var_term(0)), (1, var_term(1))]),
                    atom_over_t(None, vec![(0, var_term(0))]),
                ],
                vec![atom_over_t(None, vec![(0, var_term(0))])],
            )],
        };
        let builder = FlirProgram::from_flat_realm(&realm).expect("The realm lowers");

        crate::host::resolver::ResolvedCode::from(builder.code)
            .expect("The lowered program must resolve");
    }

    #[test]
    fn declaring_the_same_rule_twice_is_an_error() {
        let rule = rule_entry(
            "r",
            vec![scalar_var("x")],
            vec![atom_over_t(None, vec![(0, var_term(0))])],
            vec![atom_over_t(None, vec![(0, var_term(0))])],
        );
        let realm = FlatRealm {
            tables: vec![table_entry(vec![("a", builtin())])],
            rules: vec![rule.clone(), rule],
        };
        assert!(FlirProgram::from_flat_realm(&realm).is_err());
    }

    fn multi_way_join(expr: &Expr) -> &MultiWayEquiJoinExpr {
        match expr {
            Expr::Relational(rel) => match rel {
                RelExpr::MultiWayEquiJoin(join) => join,
                other => panic!("Expected a multi way equi join, got {other:?}"),
            },
            other => panic!("Expected a relational expression, got {other:?}"),
        }
    }

    fn conjunctive_query(atoms: Vec<ir::Atom>, conditions: Vec<ir::Equality>) -> ConjunctiveQuery {
        ConjunctiveQuery { atoms, conditions }
    }

    fn lit_term(value: i64) -> ir::Term {
        ir::Term::Lit {
            lit: ir::Lit::Int { value },
        }
    }

    fn equality(left: ir::Term, right: ir::Term) -> ir::Equality {
        ir::Equality { left, right }
    }

    fn assert_selection(expr: &Expr) -> &SelectionExpr {
        match maybe_assert_selection(expr) {
            Some(selection) => selection,
            None => panic!("Expected a relational selection expression, got {expr:?}"),
        }
    }

    #[test]
    fn a_single_atom_conjunctive_query_needs_no_join() {
        // There is nothing to equate across atoms, and the join operators reject
        // fewer than two relations, so the atom must come through as-is.
        let mut builder = builder_with_table(vec![("a", builtin())]);
        let query = conjunctive_query(vec![atom_over_t(None, vec![(0, var_term(0))])], vec![]);
        let (expr, bindings) = builder
            .conjunctive_query(&query, &[scalar_var("x")])
            .expect("A one-atom conjunctive query lowers");

        assert_eq!(attribute_names(assert_projection(&expr)), vec!["x"]);
        assert_eq!(bindings.len(), 1);
    }

    #[test]
    fn two_atoms_sharing_a_variable_lower_to_a_join_on_that_variable() {
        let mut builder = builder_with_table(vec![("a", builtin()), ("b", builtin())]);
        // t(x, y) and t(x, z): `x` is shared, `y` and `z` are not.
        let query = conjunctive_query(
            vec![
                atom_over_t(None, vec![(0, var_term(0)), (1, var_term(1))]),
                atom_over_t(None, vec![(0, var_term(0)), (1, var_term(2))]),
            ],
            vec![],
        );
        let vars = vec![scalar_var("x"), scalar_var("y"), scalar_var("z")];
        let (expr, bindings) = builder
            .conjunctive_query(&query, &vars)
            .expect("A two-atom conjunctive query lowers");

        let join = multi_way_join(&expr);
        assert_eq!(join.relations.len(), 2);
        assert_eq!(
            summary(&join.on),
            vec![("x".to_string(), vec![0, 1])],
            "Only the shared variable may appear in the join condition"
        );
        // `y` and `z` are still bound by the query as a whole, which is what
        // keeps them available to an enclosing antijoin.
        assert_eq!(bindings.len(), 3);
        assert!(join.validate().is_ok());
    }

    #[test]
    fn two_atoms_sharing_nothing_lower_to_a_cartesian_product() {
        let mut builder = builder_with_table(vec![("a", builtin())]);
        let query = conjunctive_query(
            vec![
                atom_over_t(None, vec![(0, var_term(0))]),
                atom_over_t(None, vec![(0, var_term(1))]),
            ],
            vec![],
        );
        let vars = vec![scalar_var("x"), scalar_var("y")];
        let (expr, _bindings) = builder
            .conjunctive_query(&query, &vars)
            .expect("Atoms sharing no variable still lower");

        assert!(
            multi_way_join(&expr).on.is_empty(),
            "An empty join condition is how a cartesian product is expressed"
        );
    }

    #[test]
    fn several_conditions_lower_to_one_selection_on_top_of_the_join() {
        // The conditions are ANDed into a single condition, so exactly one
        // selection sits on top of the join rather than one selection per
        // condition chained after another.
        let mut builder = builder_with_table(vec![("a", builtin()), ("b", builtin())]);
        // t(x, y) and t(x, z), with `y = 1` and `z = 2`.
        let query = conjunctive_query(
            vec![
                atom_over_t(None, vec![(0, var_term(0)), (1, var_term(1))]),
                atom_over_t(None, vec![(0, var_term(0)), (1, var_term(2))]),
            ],
            vec![
                equality(var_term(1), lit_term(1)),
                equality(var_term(2), lit_term(2)),
            ],
        );
        let vars = vec![scalar_var("x"), scalar_var("y"), scalar_var("z")];
        let (expr, _bindings) = builder
            .conjunctive_query(&query, &vars)
            .expect("A conjunctive query with conditions lowers");

        let selection = assert_selection(&expr);
        // What sits directly beneath the selection is the join itself, and not
        // another selection carrying the second condition.
        multi_way_join(&selection.relation);
        match &selection.condition {
            Expr::Binary(and) => {
                assert_eq!(and.operator, Operator::And);
                for side in [&and.left, &and.right] {
                    match side {
                        Expr::Binary(equality) => assert_eq!(equality.operator, Operator::Equal),
                        other => panic!("Expected an equality condition, got {other:?}"),
                    }
                }
            }
            other => panic!("Expected the two conditions to be ANDed, got {other:?}"),
        }
    }

    #[test]
    fn a_conjunctive_query_without_atoms_is_an_error() {
        let mut builder = builder_with_table(vec![("a", builtin())]);
        assert!(
            builder
                .conjunctive_query(&conjunctive_query(vec![], vec![]), &[])
                .is_err()
        );
    }

    #[test]
    fn an_atom_over_an_undeclared_entity_is_an_error() {
        let mut builder = builder_with_table(vec![("a", builtin())]);
        let atom = ir::Atom {
            entity: ir::Path::from("nonexistent"),
            row_id: None,
            values: vec![],
        };
        assert!(builder.atom(&atom, &[]).is_err());
    }

    fn scalar_var(name: &str) -> FriendlyVar {
        FriendlyVar {
            name: ir::Path::from(name),
            ty: ir::ColType::BuiltinTy {
                builtin_ty: ir::BuiltinTy::BuiltinInt,
            },
        }
    }

    fn row_id_var(name: &str) -> FriendlyVar {
        FriendlyVar {
            name: ir::Path::from(name),
            ty: ir::ColType::RowId {
                path: ir::Path::from("some_table"),
            },
        }
    }

    fn binding(var: ir::VarIdx, part: VarPart, name: &str) -> Binding {
        Binding {
            var,
            part,
            name: name.to_string(),
            // Irrelevant to join-variable and antijoin-key derivation; the
            // schema tests below assert on types via `atom` instead.
            scalar_type: ScalarType::Null,
        }
    }

    /// An [`AtomPlan`] whose relation is a stand-in: only the bindings matter to
    /// [`join_variables`] and [`antijoin_key`].
    fn plan(bindings: Vec<Binding>) -> AtomPlan {
        AtomPlan {
            relation: Expr::from(VarExpr::new("atom")),
            bindings,
        }
    }

    /// The relation indices and output name of each derived join variable.
    fn summary(variables: &[JoinVariable]) -> Vec<(String, Vec<RelationIdx>)> {
        variables
            .iter()
            .map(|variable| {
                (
                    variable.name.clone(),
                    variable
                        .occurrences
                        .iter()
                        .map(|(relation, _)| *relation)
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn a_scalar_variable_flattens_into_one_part() {
        let parts: Vec<(VarPart, String)> = scalar_var("x").parts().collect();
        assert_eq!(parts, vec![(VarPart::Scalar, "x".to_string())]);
    }

    #[test]
    fn a_row_id_variable_flattens_into_a_hash_and_a_counter_part() {
        let var = row_id_var("x");
        let parts: Vec<VarPart> = var.parts().map(|(part, _)| part).collect();
        assert_eq!(parts, vec![VarPart::RowIdHash, VarPart::RowIdCtr]);
        // The names have to differ, or the projection would collide with itself.
        let names: Vec<String> = var.parts().map(|(_, name)| name).collect();
        assert_ne!(names[0], names[1]);
        assert!(names.iter().all(|name| name.contains('x')));
    }

    #[test]
    fn a_variable_shared_by_two_atoms_becomes_one_join_variable() {
        let plans = vec![
            plan(vec![binding(0, VarPart::Scalar, "x")]),
            plan(vec![binding(0, VarPart::Scalar, "x")]),
        ];
        assert_eq!(
            summary(&join_variables(&plans)),
            vec![("x".to_string(), vec![0, 1])]
        );
    }

    #[test]
    fn a_variable_bound_by_a_single_atom_is_not_a_join_variable() {
        // It constrains nothing, and it still reaches the output through its
        // atom's schema — which is what keeps it available to the antijoin.
        let plans = vec![
            plan(vec![
                binding(0, VarPart::Scalar, "x"),
                binding(1, VarPart::Scalar, "lonely"),
            ]),
            plan(vec![binding(0, VarPart::Scalar, "x")]),
        ];
        assert_eq!(
            summary(&join_variables(&plans)),
            vec![("x".to_string(), vec![0, 1])]
        );
    }

    #[test]
    fn atoms_sharing_no_variable_yield_an_empty_join_condition() {
        let plans = vec![
            plan(vec![binding(0, VarPart::Scalar, "x")]),
            plan(vec![binding(1, VarPart::Scalar, "y")]),
        ];
        assert!(join_variables(&plans).is_empty());
    }

    #[test]
    fn a_shared_row_id_variable_yields_one_join_variable_per_half() {
        // Equality on a row id is equality on the hash *and* the counter, so the
        // two halves are two independent equality classes.
        let plans = vec![
            plan(vec![
                binding(0, VarPart::RowIdHash, "xRowIdHash"),
                binding(0, VarPart::RowIdCtr, "xRowIdCtr"),
            ]),
            plan(vec![
                binding(0, VarPart::RowIdHash, "xRowIdHash"),
                binding(0, VarPart::RowIdCtr, "xRowIdCtr"),
            ]),
        ];
        assert_eq!(
            summary(&join_variables(&plans)),
            vec![
                ("xRowIdHash".to_string(), vec![0, 1]),
                ("xRowIdCtr".to_string(), vec![0, 1]),
            ]
        );
    }

    #[test]
    fn a_variable_shared_by_three_atoms_has_three_occurrences() {
        let plans = vec![
            plan(vec![binding(0, VarPart::Scalar, "x")]),
            plan(vec![binding(0, VarPart::Scalar, "x")]),
            plan(vec![binding(0, VarPart::Scalar, "x")]),
        ];
        assert_eq!(
            summary(&join_variables(&plans)),
            vec![("x".to_string(), vec![0, 1, 2])]
        );
    }

    #[test]
    fn join_variables_are_ordered_by_flir_variable_index() {
        // Not by the order the atoms happened to bind them in, so that the same
        // input always lowers to the same plan.
        let plans = vec![
            plan(vec![
                binding(2, VarPart::Scalar, "c"),
                binding(0, VarPart::Scalar, "a"),
            ]),
            plan(vec![
                binding(0, VarPart::Scalar, "a"),
                binding(2, VarPart::Scalar, "c"),
                binding(1, VarPart::Scalar, "b"),
            ]),
            plan(vec![binding(1, VarPart::Scalar, "b")]),
        ];
        let names: Vec<String> = join_variables(&plans)
            .into_iter()
            .map(|variable| variable.name)
            .collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn derived_join_variables_satisfy_the_join_operator_invariants() {
        // The two halves of this change have to fit: whatever `join_variables`
        // derives must be constructible, which is the check that no singleton
        // and no repeated relation index slips through.
        let plans = vec![
            plan(vec![
                binding(0, VarPart::Scalar, "x"),
                binding(1, VarPart::Scalar, "only_here"),
            ]),
            plan(vec![
                binding(0, VarPart::Scalar, "x"),
                binding(2, VarPart::Scalar, "y"),
            ]),
            plan(vec![binding(2, VarPart::Scalar, "y")]),
        ];
        let on = join_variables(&plans);
        let relations = plans.into_iter().map(|plan| plan.relation).collect();
        MultiWayEquiJoinExpr::new(relations, on, None)
            .expect("Derived join variables must satisfy the operator's invariants");
    }

    #[test]
    fn the_antijoin_key_is_the_intersection_of_both_sides() {
        let left = vec![
            binding(0, VarPart::Scalar, "shared"),
            binding(1, VarPart::Scalar, "left_only"),
        ];
        let right = vec![
            binding(0, VarPart::Scalar, "shared"),
            binding(2, VarPart::Scalar, "right_only"),
        ];
        let key = antijoin_key(&left, &right);
        assert_eq!(key.len(), 1);
        assert_eq!(
            key[0],
            (
                Expr::from(VarExpr::new("shared")),
                Expr::from(VarExpr::new("shared"))
            )
        );
    }

    #[test]
    fn the_antijoin_key_matches_parts_rather_than_variables() {
        // Both sides bind variable 0, but the hash half only appears on the
        // left, so only the counter half may be compared.
        let left = vec![
            binding(0, VarPart::RowIdHash, "xRowIdHash"),
            binding(0, VarPart::RowIdCtr, "xRowIdCtr"),
        ];
        let right = vec![binding(0, VarPart::RowIdCtr, "xRowIdCtr")];
        let key = antijoin_key(&left, &right);
        assert_eq!(key.len(), 1);
        assert_eq!(key[0].0, Expr::from(VarExpr::new("xRowIdCtr")));
    }

    #[test]
    fn disjoint_sides_produce_an_empty_antijoin_key() {
        let left = vec![binding(0, VarPart::Scalar, "x")];
        let right = vec![binding(1, VarPart::Scalar, "y")];
        assert!(antijoin_key(&left, &right).is_empty());
    }

    fn translate_json_flir(file_name: &str) -> FlirProgram {
        let flat_realm = coln_flir_rs::test_utils::load_theory_from_json(file_name);
        FlirProgram::from_flat_realm(&flat_realm)
            .unwrap_or_else(|_| panic!("{file_name} is convertible to a query program"))
    }

    #[test]
    fn graph_flir() {
        let program = translate_json_flir("Graph.json");
        println!("{}", program.to_tree());
    }

    #[test]
    fn graph_of_graphs_flir() {
        let program = translate_json_flir("GraphOfGraphs.json");
        println!("{}", program.to_tree());
    }
}
