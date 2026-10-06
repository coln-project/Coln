// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module converts coln's flattened lowered intermediate representation
//! (FLIR) into a [logical program (Datalog)](LogicalProgram) and into
//! [QueryIr] which can eventually be executed by the query engine(s).

use crate::error::SyntaxError;
use crate::frontend::{self, Identifiable, LogicalProgram, RulePredicate};
use crate::host::{
    QueryIr,
    expr::{self, Literal},
    operator::Operator,
};
use crate::program::QueryProgram;
use crate::relational::{
    catalog::Catalog,
    expr::{SinkId, SourceId},
    schema::{Column, EntityRef, TableSchema},
};
use crate::scalarial::ScalarType;
use coln_flir_rs::{
    ir::{self, El, FlatRealm},
    schema::{
        BaseTable, ColnSchema, ColnSchemaWrapper, CompilerColIdx, DerivedView, NativeScalarType,
        QueryEngineCol, QueryEngineScalarType, ResolveCompilerIdxToQueryView, RuleVars,
        StoreEngineCols,
    },
};
use indexmap::IndexMap;
use std::borrow::Cow;

type BaseTableName = EntityRef;
type DerivedViewName = EntityRef;
type ConstraintName = EntityRef;

/// Coln's FLIR frontend's [`QueryProgram`]: what a [`FlatRealm`] lowers to.
///
/// The [`Catalog`] half is served straight out of [`base_tables`](Self::base_tables),
/// which stores FLIR's own richer [`BaseTableSchema`] which includes the schema
/// view according to coln-compiler and coln-store next coln-query's.
#[derive(Debug)]
pub struct FlirProgram {
    /// The logical query program which is essentially Datalog.
    predicates: Vec<RulePredicate<FlirRule>>,
    /// The raw, that is, unresolved and unoptimized, query IR statements
    /// restating [Self::predicates] in relational algebra.
    code: QueryIr,
    /// The declared base tables. Doubles as this program's [`Catalog`]: every
    /// [`SourceExpr`] the lowering mints names one of these.
    base_tables: IndexMap<BaseTableName, PredicateMeta<BaseTable>>,
    /// Any materialized, maintained, derived view. Doubles as this program's
    /// [`Catalog`] but for adhoc-queries, which are allowed to read from the
    /// materialized views, too, as opposed to the incrementally-maintained
    /// queries defined in here.
    ///
    /// This doubles as the set of derived views an [`Atom`] may reference, so
    /// that what [`definition_entries`](Self::definition_entries) writes is
    /// exactly what [`derived_view_var_expr`](Self::derived_view_var_expr)
    /// reads.
    derived_views: IndexMap<DerivedViewName, PredicateMeta<DerivedView>>,
    /// The constraints the program itself defines, that is, one per declared
    /// constraint, which is an enforced or monitored rule.
    constraints: IndexMap<ConstraintName, ConstraintMeta>,
}

#[derive(Debug, Clone)]
pub struct PredicateMeta<Marker> {
    coln_schema: ColnSchema<Marker>,
    output_schema: TableSchema,
}

#[derive(Debug)]
pub struct ConstraintMeta {
    kind: ir::RuleVariant,
    output_schema: TableSchema,
}

impl ConstraintMeta {
    fn new(kind: ir::RuleVariant, output_schema: TableSchema) -> Self {
        ConstraintMeta {
            kind,
            output_schema,
        }
    }
    pub fn kind(&self) -> ir::RuleVariant {
        self.kind
    }
    pub fn schema(&self) -> &TableSchema {
        &self.output_schema
    }
}

/// Projects FLIR's schema down to the one thing the layers below share:
/// the query engine's columns, and the table's key(s) restated over them.
///
/// The implicit row id leads the list of keys: it is the only key a base table
/// is guaranteed to have and to be unique on, so a backend that can index by
/// just one key (DBSP) picks it by taking the first.
impl From<&ColnSchema<BaseTable>> for TableSchema {
    fn from(value: &ColnSchema<BaseTable>) -> Self {
        let columns = value
            .query_cols()
            .inner()
            .iter()
            .map(|col| Column::new(col.name().to_string(), *col.ty()))
            .collect();
        let row_id_key = value
            .resolve_query_cols(CompilerColIdx::for_row_id())
            .map(|(idx, _)| idx.0)
            .collect();
        let declared_keys = value.primary_keys().iter().filter_map(|key| {
            if !key.is_empty() {
                Some(
                    key.iter()
                        .flat_map(|idx| value.resolve_query_cols(*idx).map(|(idx, _)| idx.0))
                        .collect(),
                )
            } else {
                // The compiler reports an empty primary key to denote that
                // there can be at most one row. This is a coln-store concern,
                // coln-query ignores this, as this provides no way to uniquely
                // identify an element.
                None
            }
        });
        TableSchema::new(
            EntityRef::from(value.name()),
            columns,
            std::iter::once(row_id_key).chain(declared_keys).collect(),
        )
    }
}

/// For a derived view, we lack the implicit row id primary key.
impl From<&ColnSchema<DerivedView>> for TableSchema {
    fn from(value: &ColnSchema<DerivedView>) -> Self {
        let columns = value
            .query_cols()
            .inner()
            .iter()
            .map(|col| Column::new(col.name().to_string(), *col.ty()))
            .collect();
        let declared_keys = value.primary_keys().iter().filter_map(|key| {
            if !key.is_empty() {
                Some(
                    key.iter()
                        .flat_map(|idx| match idx {
                            CompilerColIdx::RowId => {
                                panic!("Extra primary key specified as RowId idx")
                            }
                            CompilerColIdx::Column(idx) => {
                                value.resolve_query_cols(*idx).map(|(idx, _)| idx.0)
                            }
                        })
                        .collect(),
                )
            } else {
                // The compiler reports an empty primary key to denote that
                // there can be at most one row. This is a coln-store concern,
                // coln-query ignores this, as this provides no way to uniquely
                // identify an element.
                None
            }
        });
        TableSchema::new(
            EntityRef::from(value.name()),
            columns,
            declared_keys.collect(),
        )
    }
}

impl FlirProgram {
    pub fn from_flat_realm(flat_realm: &FlatRealm) -> Result<Self, SyntaxError> {
        let mut ctx = FlirContext::default();

        let edb_rules: Vec<FlirRule> = flat_realm
            .tables
            .iter()
            .flat_map(|table| {
                let edb_predicate = ctx.feed_table_declaration(table)?;
                Some(FlirRule::from(edb_predicate))
            })
            .collect();

        let derived_view_rules: Vec<FlirRule> = flat_realm
            .definitions
            .iter()
            .map(|definition| {
                let rule = FlirChasedRule::new(definition, &mut ctx)?;
                Ok(FlirRule::from(rule))
            })
            .collect::<Result<_, SyntaxError>>()?;

        let constraint_rules: Vec<FlirRule> = flat_realm
            .rules
            .iter()
            .flat_map(|rule| {
                if rule.rule.consequents.is_empty() {
                    None
                } else {
                    Some(FlirConstraint::new(rule, &mut ctx).map(FlirRule::from))
                }
            })
            .collect::<Result<_, SyntaxError>>()?;

        let mut rules = edb_rules;
        rules.extend(derived_view_rules);
        rules.extend(constraint_rules);

        let predicates = RulePredicate::group(ctx.declarations(), rules).map_err(|unmatched| {
            SyntaxError::new(format!("Unmatched rules with no definand {unmatched}"))
        })?;

        fn remap<P, M>(map: impl IntoIterator<Item = (P, M)>) -> IndexMap<EntityRef, M>
        where
            EntityRef: From<P>,
        {
            map.into_iter()
                .map(|(path, meta)| (EntityRef::from(path), meta))
                .collect()
        }

        let mut program = FlirProgram {
            predicates,
            code: QueryIr::new(vec![]),
            base_tables: remap(ctx.base_tables),
            derived_views: remap(ctx.derived_views),
            constraints: remap(ctx.constraints),
        };

        let execution_order = program.verify()?;
        program.code = program.prepare(execution_order)?;

        Ok(program)
    }

    pub fn constraint_meta(&self, sink: &SinkId) -> Option<&ConstraintMeta> {
        self.constraints.get(&ConstraintName::from(sink))
    }

    pub fn derived_view_meta(&self, sink: &SinkId) -> Option<&PredicateMeta<DerivedView>> {
        self.derived_views.get(&DerivedViewName::from(sink))
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
            .map(|meta| Cow::Borrowed(&meta.output_schema))
    }
}

impl LogicalProgram for FlirProgram {
    type Predicate = RulePredicate<FlirRule>;

    fn predicates(&self) -> impl Iterator<Item = &Self::Predicate> {
        self.predicates.iter()
    }
}

impl QueryProgram for FlirProgram {
    fn code(&self) -> &QueryIr {
        &self.code
    }

    fn take_code(&mut self) -> QueryIr {
        std::mem::take(&mut self.code)
    }
}

impl frontend::Identifier for ir::Path {}
impl frontend::Identifier for &ir::Path {}

#[derive(Default)]
struct FlirContext {
    base_tables: IndexMap<ir::Path, PredicateMeta<BaseTable>>,
    derived_views: IndexMap<ir::Path, PredicateMeta<DerivedView>>,
    constraints: IndexMap<ir::Path, ConstraintMeta>,
    rule_vars: Option<RuleVars>,
}

impl FlirContext {
    fn enter_rule_scope<'a>(
        &'a mut self,
        vars: &[(ir::ColName, ir::ColType)],
    ) -> RuleScopeGuard<'a> {
        self.rule_vars = Some(RuleVars::new(vars));
        RuleScopeGuard { inner: self }
    }
    fn schema(&self, entity: &ir::Path) -> Option<impl ResolveCompilerIdxToQueryView> {
        self.base_tables
            .get(entity)
            .map(|meta| ColnSchemaWrapper::BaseTable(&meta.coln_schema))
            .or_else(|| {
                Some(ColnSchemaWrapper::DerivedView(
                    self.derived_views
                        .get(entity)
                        .map(|meta| &meta.coln_schema)?,
                ))
            })
    }
    fn feed_table_declaration<'a>(
        &mut self,
        declaration: &'a ir::TableEntry,
    ) -> Option<FlirEdbPredicate<'a>> {
        let entity = &declaration.path;
        if let Some(edb_predicate) = FlirEdbPredicate::new(declaration, self) {
            return Some(edb_predicate);
        }
        if let Some(derived_view_schema) = Option::<ColnSchema<DerivedView>>::from(declaration) {
            self.derived_views.insert(
                entity.clone(),
                PredicateMeta {
                    output_schema: TableSchema::from(&derived_view_schema),
                    coln_schema: derived_view_schema,
                },
            );
            return None;
        }
        panic!(
            "Table declaration for '{entity}' neither valid as a base table nor as a derived view"
        )
    }
    fn declarations(&self) -> impl Iterator<Item = (ir::Path, Vec<Column>)> {
        let base_table_declarations = self
            .base_tables
            .iter()
            .map(|(path, meta)| (path, &meta.output_schema));
        let derived_view_declarations = self
            .derived_views
            .iter()
            .map(|(path, meta)| (path, &meta.output_schema));
        let constraint_declarations = self
            .constraints
            .iter()
            .map(|(path, meta)| (path, &meta.output_schema));
        base_table_declarations
            .chain(derived_view_declarations.chain(constraint_declarations))
            .map(|(path, schema)| (path.clone(), schema.columns().to_vec()))
    }
}

struct RuleScopeGuard<'a> {
    inner: &'a mut FlirContext,
}

impl RuleScopeGuard<'_> {
    fn rule_vars(&self) -> &RuleVars {
        self.inner.rule_vars.as_ref().expect("within scope guard")
    }
}

trait ResolveFlirVarIdx {
    fn resolve_flir_var_idx(&self, idx: ir::VarIdx) -> (FlirVar, Option<FlirVar>);
    fn query_vars(&self) -> impl Iterator<Item = FlirVar>;
}

impl ResolveFlirVarIdx for RuleScopeGuard<'_> {
    fn resolve_flir_var_idx(&self, idx: ir::VarIdx) -> (FlirVar, Option<FlirVar>) {
        let mut vars = self.rule_vars().resolve_query_cols(idx).map(FlirVar::from);
        (vars.next().expect("At least one variable"), vars.next())
    }
    fn query_vars(&self) -> impl Iterator<Item = FlirVar> {
        self.rule_vars()
            .query_cols()
            .inner()
            .iter()
            .map(FlirVar::from)
    }
}

impl std::ops::Deref for RuleScopeGuard<'_> {
    type Target = FlirContext;
    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl std::ops::DerefMut for RuleScopeGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

impl Drop for RuleScopeGuard<'_> {
    fn drop(&mut self) {
        self.inner.rule_vars = None;
    }
}

/// The mother type combining [`FlirEdbPredicate`], [`FlirChasedRule`], and
/// [`FlirConstraint`] to expose them all as a [`frontend::Rule`].
#[derive(Debug)]
pub struct FlirRule {
    id: ir::Path,
    head: FlirAtom,
    body: Vec<FlirAtom>,
    conditions: Vec<FlirCond>,
}

impl From<FlirEdbPredicate<'_>> for FlirRule {
    fn from(predicate: FlirEdbPredicate<'_>) -> Self {
        FlirRule {
            id: predicate.id().clone(),
            head: predicate.head,
            body: Vec::new(),
            conditions: Vec::new(),
        }
    }
}

impl From<FlirChasedRule<'_>> for FlirRule {
    fn from(rule: FlirChasedRule<'_>) -> Self {
        FlirRule {
            id: rule.id().clone(),
            head: rule.head,
            body: rule.body,
            conditions: rule.conditions,
        }
    }
}

impl From<FlirConstraint<'_>> for FlirRule {
    fn from(constraint: FlirConstraint<'_>) -> Self {
        FlirRule {
            id: constraint.id().clone(),
            head: constraint.head,
            body: constraint.body,
            conditions: constraint.conditions,
        }
    }
}

impl Identifiable for FlirRule {
    type Identifier = ir::Path;
    fn id(&self) -> &Self::Identifier {
        &self.id
    }
}

impl frontend::Rule for FlirRule {
    type Atom = FlirAtom;
    type Cond = FlirCond;

    fn head(&self) -> &Self::Atom {
        &self.head
    }

    fn atoms(&self) -> impl Iterator<Item = &Self::Atom> {
        self.body.iter()
    }

    fn conditions(&self) -> impl Iterator<Item = &Self::Cond> {
        self.conditions.iter()
    }
}

#[derive(Debug)]
struct FlirEdbPredicate<'a> {
    base_table: &'a ir::TableEntry,
    head: FlirAtom,
}

impl<'a> FlirEdbPredicate<'a> {
    fn new(base_table: &'a ir::TableEntry, ctx: &mut FlirContext) -> Option<FlirEdbPredicate<'a>> {
        let schema = Option::<ColnSchema<BaseTable>>::from(base_table)?;
        let head = FlirAtom::new_head(
            base_table.path.clone(),
            schema
                .query_cols()
                .inner()
                .iter()
                .map(|col| frontend::Bind::Var(FlirVar::from(col)))
                .collect(),
        );
        ctx.base_tables.insert(
            base_table.path.clone(),
            PredicateMeta {
                output_schema: TableSchema::from(&schema),
                coln_schema: schema,
            },
        );
        Some(Self { base_table, head })
    }
}

impl Identifiable for FlirEdbPredicate<'_> {
    type Identifier = ir::Path;
    fn id(&self) -> &Self::Identifier {
        &self.base_table.path
    }
}

#[derive(Debug)]
struct FlirChasedRule<'a> {
    rule: &'a ir::DefinitionEntry,
    head: FlirAtom,
    body: Vec<FlirAtom>,
    conditions: Vec<FlirCond>,
}

impl<'a> FlirChasedRule<'a> {
    /// Returns `None` if the [rule's definand](ir::Definition::definand) has
    /// not been declared in the `ctx`.
    fn new(rule: &'a ir::DefinitionEntry, ctx: &mut FlirContext) -> Result<Self, SyntaxError> {
        let definand = &rule.definition.definand;
        let schema = ctx
            .derived_views
            .get(definand)
            .ok_or_else(|| SyntaxError::new(format!("Chased rule specifies unknown {definand}")))?;
        assert_eq!(
            rule.definition.arguments.len(),
            schema.coln_schema.compiler_cols().inner().len(),
            "Number of supplied arguments does not match the definand's definition"
        );
        let rule_scope = ctx.enter_rule_scope(&rule.definition.vars);
        let head = FlirAtom::new_head(
            definand.clone(),
            rule.definition
                .arguments
                .iter()
                .flat_map(|argument| resolve_element(argument, &rule_scope))
                .collect(),
        );
        let (body, conditions) = resolve_props(&rule.definition.antecedents, false, &rule_scope)?;
        Ok(FlirChasedRule {
            rule,
            head,
            body,
            conditions,
        })
    }
}

impl Identifiable for FlirChasedRule<'_> {
    type Identifier = ir::Path;
    fn id(&self) -> &Self::Identifier {
        &self.rule.path
    }
}

#[derive(Debug)]
struct FlirConstraint<'a> {
    rule: &'a ir::RuleEntry,
    head: FlirAtom,
    body: Vec<FlirAtom>,
    conditions: Vec<FlirCond>,
}

impl<'a> FlirConstraint<'a> {
    fn new(rule: &'a ir::RuleEntry, ctx: &mut FlirContext) -> Result<Self, SyntaxError> {
        let mut rule_scope = ctx.enter_rule_scope(&rule.rule.vars);
        let (schema_columns, head_fields): (Vec<_>, Vec<_>) = rule_scope
            .query_vars()
            .map(|var| {
                (
                    Column::from(var.clone()),
                    frontend::Bind::<FlirVar, FlirLit>::Var(var),
                )
            })
            .collect();
        let output_schema = TableSchema::new(EntityRef::from(&rule.path), schema_columns, vec![]);
        rule_scope.constraints.insert(
            rule.path.clone(),
            ConstraintMeta::new(rule.rule.rule_variant, output_schema),
        );
        // TODO: Verify that all vars are covered by the antecedent, or if not,
        // filter the vars to only include the antecedent's vars.
        let head = FlirAtom::new_head(rule.path.clone(), head_fields);
        let (mut atoms, mut conditions) =
            resolve_props(&rule.rule.antecedents, false, &rule_scope)?;
        let (negated_atoms, negative_conditions) =
            resolve_props(&rule.rule.consequents, true, &rule_scope)?;
        atoms.extend(negated_atoms);
        conditions.extend(negative_conditions);
        Ok(Self {
            rule,
            head,
            body: atoms,
            conditions,
        })
    }
}

impl Identifiable for FlirConstraint<'_> {
    type Identifier = ir::Path;
    fn id(&self) -> &Self::Identifier {
        &self.rule.path
    }
}

#[derive(Debug)]
pub struct FlirAtom {
    name: ir::Path,
    negated: bool,
    /// Note: The bindings are sparsely defined.
    bindings: Vec<(usize, frontend::Bind<FlirVar, FlirLit>)>,
}

impl FlirAtom {
    fn new_head(name: ir::Path, fields: Vec<frontend::Bind<FlirVar, FlirLit>>) -> FlirAtom {
        FlirAtom {
            name,
            negated: false,
            bindings: fields.into_iter().enumerate().collect(),
        }
    }
    fn from_atom(
        atom: &ir::Atom,
        negated: bool,
        resolver: &RuleScopeGuard,
    ) -> Result<FlirAtom, SyntaxError> {
        let mut bindings = Vec::with_capacity(
            if atom.row_id.is_some() {
                StoreEngineCols::ROW_ID_COLS
            } else {
                0
            } + atom.values.len(),
        );
        let entity = &atom.entity;
        let schema = resolver
            .schema(entity)
            .ok_or_else(|| SyntaxError::new(format!("No schema for {entity}")))?;
        let mut bind = |idx: CompilerColIdx, el: &ir::El| -> Result<(), SyntaxError> {
            // The index comes from the query schema but the name comes from the variable.
            let query_cols = schema
                .resolve_query_cols(idx)
                .ok_or_else(|| SyntaxError::new("Invalid compiler index"))?;
            let vars = resolve_element(el, resolver);
            assert_eq!(
                query_cols.len(),
                vars.len(),
                "Mismatch between resolved vars and resolved columns"
            );
            bindings.extend(query_cols.map(|(idx, _col)| idx.0).zip(vars));
            Ok(())
        };
        if let Some(row_id) = &atom.row_id {
            bind(CompilerColIdx::RowId, row_id)?;
        }
        for binding in &atom.values {
            bind(CompilerColIdx::Column(binding.column), &binding.term)?;
        }
        Ok(FlirAtom {
            name: entity.clone(),
            negated,
            bindings,
        })
    }
}

impl Identifiable for FlirAtom {
    type Identifier = ir::Path;
    fn id(&self) -> &Self::Identifier {
        &self.name
    }
}

impl frontend::Atom for FlirAtom {
    type Var = FlirVar;
    type Lit = FlirLit;

    fn is_positive(&self) -> bool {
        !self.negated
    }

    fn bindings(&self) -> impl Iterator<Item = (usize, frontend::Bind<&Self::Var, &Self::Lit>)> {
        self.bindings
            .iter()
            .map(|(idx, bind)| (*idx, bind.as_ref()))
    }
}

fn resolve_props(
    props: &[ir::Prop],
    negated: bool,
    resolver: &RuleScopeGuard,
) -> Result<(Vec<FlirAtom>, Vec<FlirCond>), SyntaxError> {
    let (body, conditions): (Vec<FlirAtom>, Vec<FlirCond>) = props.iter().try_fold(
        (Vec::new(), Vec::new()),
        |(mut body, mut conditions), prop| {
            match prop {
                ir::Prop::Atom { atom } => body.push(FlirAtom::from_atom(atom, negated, resolver)?),
                ir::Prop::Eq { equality } => conditions.extend(FlirCond::new(
                    if negated {
                        Operator::NotEqual
                    } else {
                        Operator::Equal
                    },
                    &equality.left,
                    &equality.right,
                    resolver,
                )),
            };
            Ok((body, conditions))
        },
    )?;
    Ok((body, conditions))
}

struct MaybePair<T>(pub T, pub Option<T>);

impl<T> MaybePair<T> {
    fn single(first: T) -> Self {
        Self(first, None)
    }
    fn maybe(pair: (T, Option<T>)) -> Self {
        Self(pair.0, pair.1)
    }
    fn into_inner(self) -> (T, Option<T>) {
        (self.0, self.1)
    }
    fn len(&self) -> usize {
        1 + (self.1.is_some() as usize)
    }
}

impl<T> IntoIterator for MaybePair<T> {
    type Item = T;
    type IntoIter = std::iter::Chain<std::iter::Once<T>, std::option::IntoIter<T>>;

    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(self.0).chain(self.1)
    }
}

fn resolve_element(
    el: &ir::El,
    resolver: &impl ResolveFlirVarIdx,
) -> MaybePair<frontend::Bind<FlirVar, FlirLit>> {
    match el {
        El::Lit { lit } => MaybePair::single(frontend::Bind::Lit(FlirLit { inner: lit.clone() })),
        El::Var { index } => {
            let (first, second) = resolver.resolve_flir_var_idx(*index);
            MaybePair::maybe((frontend::Bind::Var(first), second.map(frontend::Bind::Var)))
        }
    }
}

#[derive(Debug)]
pub struct FlirCond {
    operator: Operator,
    left: frontend::Bind<FlirVar, FlirLit>,
    right: frontend::Bind<FlirVar, FlirLit>,
}

impl FlirCond {
    /// A single condition the FLIR can turn out to be two conditions because
    /// of row ids spreading into two variables.
    fn new(
        operator: Operator,
        left: &ir::El,
        right: &ir::El,
        resolver: &impl ResolveFlirVarIdx,
    ) -> MaybePair<Self> {
        let (left, left2) = resolve_element(left, resolver).into_inner();
        let (right, right2) = resolve_element(right, resolver).into_inner();
        let second = match (left2, right2) {
            // Neither left nor right is a row id variable (or row id literal).
            // One condition is enough.
            (None, None) => None,
            // Left is a row id variable, spreading into two variables.
            // This case should not happen, as comparing a row id variable
            // to a non row id literal is invalid.
            (Some(left), None) => Some(Self {
                operator,
                left,
                right: right.clone(),
            }),
            // Right is a row id variable, spreading into two variables.
            // This case should not happen, as comparing a row id variable
            // to a non row id literal is invalid.
            (None, Some(right)) => Some(Self {
                operator,
                left: left.clone(),
                right,
            }),
            // We compare two variables which are row ids. We need a second
            // condition.
            (Some(left), Some(right)) => Some(Self {
                operator,
                left,
                right,
            }),
        };
        let first = Self {
            operator,
            left,
            right,
        };
        MaybePair::maybe((first, second))
    }
}

impl frontend::Cond for FlirCond {
    type Var = FlirVar;
    type Lit = FlirLit;

    fn operator(&self) -> impl Into<Operator> {
        self.operator
    }
    fn left(&self) -> frontend::Bind<&Self::Var, &Self::Lit> {
        self.left.as_ref()
    }
    fn right(&self) -> frontend::Bind<&Self::Var, &Self::Lit> {
        self.right.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct FlirVar {
    name: ir::Path,
    ty: ScalarType,
}

impl From<&QueryEngineCol> for FlirVar {
    fn from(value: &QueryEngineCol) -> Self {
        Self {
            name: value.name().clone(),
            ty: (*value.ty()).into(),
        }
    }
}

impl From<FlirVar> for Column {
    fn from(value: FlirVar) -> Self {
        Column::new(value.name, value.ty)
    }
}

impl frontend::Identifiable for FlirVar {
    type Identifier = ir::Path;

    fn id(&self) -> &Self::Identifier {
        &self.name
    }
}

impl frontend::TypedVar for FlirVar {
    fn ty(&self) -> ScalarType {
        self.ty
    }
}

#[derive(Debug, Clone)]
pub struct FlirLit {
    inner: ir::Lit,
}

impl From<&ir::Lit> for FlirLit {
    fn from(value: &ir::Lit) -> Self {
        Self {
            inner: value.clone(),
        }
    }
}

impl frontend::Lit for FlirLit {
    fn to_literal(&self) -> expr::Literal {
        match &self.inner {
            ir::Lit::Int { value } => expr::Literal::Iint(*value as i64),
            ir::Lit::String { value } => expr::Literal::String(value.clone()),
        }
    }
}

impl From<&ir::Path> for EntityRef {
    fn from(value: &ir::Path) -> Self {
        EntityRef::from(value.to_string())
    }
}

impl From<ir::Path> for EntityRef {
    fn from(value: ir::Path) -> Self {
        EntityRef::from(value.0)
    }
}

impl From<&ir::Lit> for Literal {
    fn from(value: &ir::Lit) -> Self {
        match value {
            ir::Lit::Int { value } => Literal::Iint((*value).into()),
            ir::Lit::String { value } => Literal::String(value.clone()),
        }
    }
}

impl From<NativeScalarType> for ScalarType {
    fn from(value: NativeScalarType) -> Self {
        match value {
            NativeScalarType::Iint => ScalarType::Iint,
            NativeScalarType::Uint => ScalarType::Uint,
            NativeScalarType::String => ScalarType::String,
        }
    }
}

impl From<QueryEngineScalarType> for ScalarType {
    fn from(value: QueryEngineScalarType) -> Self {
        match value {
            // A row id's two halves reach the query engine as plain unsigned
            // integers, so every query-engine type is a native one by this
            // point.
            QueryEngineScalarType::Native(native) => ScalarType::from(native),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn translate_json_flir(file_name: &str) -> FlirProgram {
        let flat_realm = coln_flir_rs::test_utils::load_theory_from_json(file_name);
        FlirProgram::from_flat_realm(&flat_realm)
            .unwrap_or_else(|err| panic!("{file_name} is convertible to a query program {err}"))
    }

    #[test]
    fn graph_flir() {
        let program = translate_json_flir("GraphRealm.json");
        println!("{}", program.to_tree());
    }

    #[test]
    fn graph_of_graphs_flir() {
        let program = translate_json_flir("GraphOfGraphsRealm.json");
        println!("{}", program.to_tree());
    }

    #[test]
    fn triangle_flir() {
        let program = translate_json_flir("TriangleRealm.json");
        println!("{}", program.to_tree());
    }

    #[test]
    fn transitive_closure_flir() {
        let program = translate_json_flir("TransitiveClosureRealm.json");
        println!("{}", program.to_tree());
    }

    #[test]
    fn transitive_closure_set_flir() {
        let program = translate_json_flir("TransitiveClosureSetRealm.json");
        println!("{}", program.to_tree());
    }
}
