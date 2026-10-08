// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module converts coln's flattened lowered intermediate representation
//! (FLIR) into a [logical program (Datalog)](LogicalProgram) and into
//! [QueryIr] which can eventually be executed by the query engine(s).

use crate::error::{Frame, SyntaxError};
use crate::frontend::{self, Identifiable, LogicalProgram, Prepared, RulePredicate};
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
use std::collections::HashSet;

type BaseTableName = EntityRef;
type DerivedViewName = EntityRef;
type ConstraintName = EntityRef;

/// Coln's FLIR frontend's [`QueryProgram`]: what a [`FlatRealm`] lowers to,
/// ready to run.
///
/// The only way to one is [`new`](Self::new), which takes [`Prepared`] code
/// only, so a `FlirProgram`'s code always stems from a verified program. For a
/// program that may be invalid, stop at its [`FlirLogicalProgram`].
///
/// The [`Catalog`] half is served straight out of the logical program's
/// [`base_tables`](FlirLogicalProgram::base_tables), which stores FLIR's own
/// richer [`ColnSchema<BaseTable>`](ColnSchema) which includes the schema
/// view according to coln-compiler and coln-store next coln-query's.
#[derive(Debug)]
pub struct FlirProgram {
    logical: FlirLogicalProgram,
    /// The raw, that is, unresolved and unoptimized, query IR statements
    /// restating the logical program in relational algebra.
    code: QueryIr,
}

/// What a [`FlatRealm`] lowers to as a [`LogicalProgram`], that is, before
/// it is checked and translated into a [`FlirProgram`].
///
/// Its [analysis](LogicalProgram::analyze) can be inspected, and drawn, even
/// if the program turns out invalid.
#[derive(Debug)]
pub struct FlirLogicalProgram {
    /// The logical query program which is essentially Datalog.
    predicates: Vec<RulePredicate<FlirRule>>,
    /// The declared base tables. Doubles as the [`FlirProgram`]'s [`Catalog`]: every
    /// [`SourceExpr`](crate::relational::expr::SourceExpr) the lowering mints
    /// names one of these.
    base_tables: IndexMap<BaseTableName, PredicateMeta<BaseTable>>,
    /// Any materialized, maintained, derived view. Doubles as this program's
    /// [`Catalog`] but for adhoc-queries, which are allowed to read from the
    /// materialized views, too, as opposed to the incrementally-maintained
    /// queries defined in here.
    ///
    /// This doubles as the set of derived views an [`Atom`](ir::Atom) may
    /// reference, and as what [`derived_view_meta`](Self::derived_view_meta)
    /// tells a derived view's sink apart by.
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
    /// Lowers `flat_realm` all the way: [`FlirLogicalProgram::from_flat_realm`],
    /// then [verification](LogicalProgram::verify) and
    /// [translation](LogicalProgram::prepare), then [`new`](Self::new).
    pub fn from_flat_realm(flat_realm: &FlatRealm) -> Result<Self, SyntaxError> {
        let logical = FlirLogicalProgram::from_flat_realm(flat_realm)?;
        let analysis = logical.verify().map_err(|rejected| rejected.error)?;
        let code = logical.prepare(analysis)?;
        Ok(Self::new(logical, code))
    }

    /// Pairs `logical` with the `code` its [`prepare`](LogicalProgram::prepare)
    /// returned. Takes [`Prepared`] code only, so the code is known to stem
    /// from a verified program. That it stems from `logical`, rather than from
    /// another program, is up to the caller.
    ///
    /// Translating first and pairing after is what lets the caller keep the
    /// [analysis](LogicalProgram::analyze) to itself until then, to inspect
    /// or draw it, without building it twice.
    pub fn new(logical: FlirLogicalProgram, code: Prepared) -> Self {
        FlirProgram {
            logical,
            code: code.into_code(),
        }
    }

    pub fn logical(&self) -> &FlirLogicalProgram {
        &self.logical
    }
}

impl FlirLogicalProgram {
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
                let rule = FlirChasedRule::new(definition, &mut ctx)
                    .map_err(|error| error.within(rule_frame(&definition.path)))?;
                Ok(FlirRule::from(rule))
            })
            .collect::<Result<_, SyntaxError>>()?;

        let constraint_rules: Vec<FlirRule> = flat_realm
            .rules
            .iter()
            .filter(|rule| !rule.rule.consequents.is_empty())
            .map(|rule| {
                FlirConstraint::new(rule, &mut ctx)
                    .map(FlirConstraint::into_rules)
                    .map_err(|error| error.within(rule_frame(&rule.path)))
            })
            .collect::<Result<Vec<_>, SyntaxError>>()?
            .into_iter()
            .flatten()
            .collect();

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

        Ok(FlirLogicalProgram {
            predicates,
            base_tables: remap(ctx.base_tables),
            derived_views: remap(ctx.derived_views),
            constraints: remap(ctx.constraints),
        })
    }

    pub fn constraint_meta(&self, sink: &SinkId) -> Option<&ConstraintMeta> {
        self.constraints.get(&ConstraintName::from(sink))
    }

    pub fn derived_view_meta(&self, sink: &SinkId) -> Option<&PredicateMeta<DerivedView>> {
        self.derived_views.get(&DerivedViewName::from(sink))
    }
}

impl Catalog for FlirProgram {
    /// Projects FLIR's [`ColnSchema<BaseTable>`](ColnSchema) down to the
    /// [`TableSchema`] a plan needs, on demand. [`Cow::Owned`] rather than a
    /// borrow precisely so that the richer schema stays the only stored copy.
    ///
    /// Only base tables answer here: a rule's output is bound to a host variable
    /// and referenced by [`VarExpr`](expr::VarExpr), never by a
    /// [`SourceExpr`](crate::relational::expr::SourceExpr), so
    /// [`derived_views`](FlirLogicalProgram::derived_views) is no part of the
    /// catalog.
    fn source_schema(&self, id: &SourceId) -> Option<Cow<'_, TableSchema>> {
        self.logical
            .base_tables
            .get(&BaseTableName::from(id))
            .map(|meta| Cow::Borrowed(&meta.output_schema))
    }
}

impl LogicalProgram for FlirLogicalProgram {
    type Predicate = RulePredicate<FlirRule>;

    fn predicates(&self) -> impl Iterator<Item = &Self::Predicate> {
        self.predicates.iter()
    }

    /// Exactly the derived views and the constraints, that is, what
    /// [`interpret_outputs`](super::ColnQuery) knows how to report. A helper
    /// predicate, such as a constraint's consequent, is neither.
    fn is_output(&self, predicate: &Self::Predicate) -> bool {
        let entity = EntityRef::from(predicate.id());
        self.derived_views.contains_key(&entity) || self.constraints.contains_key(&entity)
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
    /// The helper predicates deriving the constraints' consequents, with their
    /// columns. See [`FlirConstraint`].
    consequents: IndexMap<ir::Path, Vec<Column>>,
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
        let consequent_declarations = self
            .consequents
            .iter()
            .map(|(path, columns)| (path.clone(), columns.clone()));
        base_table_declarations
            .chain(derived_view_declarations.chain(constraint_declarations))
            .map(|(path, schema)| (path.clone(), schema.columns().to_vec()))
            .chain(consequent_declarations)
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
        let schema = ctx.derived_views.get(definand).ok_or_else(|| {
            SyntaxError::new(format!("rule derives undeclared predicate '{definand}'"))
        })?;
        let arguments = rule.definition.arguments.len();
        let columns = schema.coln_schema.compiler_cols().inner().len();
        if arguments != columns {
            return Err(SyntaxError::new(format!(
                "head supplies {arguments} argument(s) to '{definand}', which has {columns} column(s)"
            )));
        }
        let rule_scope = ctx.enter_rule_scope(&rule.definition.vars);
        let head = FlirAtom::new_head(
            definand.clone(),
            rule.definition
                .arguments
                .iter()
                .flat_map(|argument| resolve_element(argument, &rule_scope))
                .collect(),
        );
        let (body, conditions) = resolve_propositions(&rule.definition.antecedents, &rule_scope)?;
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

/// A constraint `antecedent ⇒ consequent`, as the rules deriving its
/// violations: the bindings of the antecedent for which the consequent does
/// not hold.
///
/// The consequent is negated as a whole, `¬(c₁ ∧ … ∧ cₙ)`, which no single rule
/// body can express: negating each proposition on its own is `¬c₁ ∧ … ∧ ¬cₙ`.
/// So the consequent becomes a helper predicate of its own, named
/// `<rule>#consequent`, and the constraint's rule negates that one atom:
///
/// ```text
/// m#consequent(x) :- t(_, _, x), x == 1.
/// m(x)            :- t(_, _, x), !m#consequent(x).
/// ```
///
/// The helper's columns are the antecedent's variables its body binds, which
/// are what the negated atom matches on. A variable only the consequent binds
/// is projected away, which makes it existential.
///
/// The helper's body is the consequent alone if that stands as a rule of its
/// own: it has an atom, and its atoms bind every variable its conditions use.
/// Otherwise, as for `∀x. t(x) ⇒ x == 1`, the antecedent joins the body. That
/// changes nothing, as the constraint's rule requires the antecedent anyway,
/// but costs computing it twice, which is why it is not done throughout.
#[derive(Debug)]
struct FlirConstraint<'a> {
    rule: &'a ir::RuleEntry,
    head: FlirAtom,
    body: Vec<FlirAtom>,
    conditions: Vec<FlirCond>,
    /// The helper rule deriving the consequent.
    consequent: FlirRule,
}

impl<'a> FlirConstraint<'a> {
    fn new(rule: &'a ir::RuleEntry, ctx: &mut FlirContext) -> Result<Self, SyntaxError> {
        let mut rule_scope = ctx.enter_rule_scope(&rule.rule.vars);
        let (mut body, conditions) = resolve_propositions(&rule.rule.antecedents, &rule_scope)?;
        let (consequent_atoms, consequent_conditions) =
            resolve_propositions(&rule.rule.consequents, &rule_scope)?;

        let (helper_atoms, helper_conditions) =
            match stands_alone(&consequent_atoms, &consequent_conditions) {
                true => (consequent_atoms, consequent_conditions),
                false => (
                    body.iter().cloned().chain(consequent_atoms).collect(),
                    conditions
                        .iter()
                        .cloned()
                        .chain(consequent_conditions)
                        .collect(),
                ),
            };
        // A violation is a binding of the antecedent's variables, so those are
        // the constraint's columns: a variable only the consequent binds is
        // existential and has no value a violation could carry. Of them, the
        // helper exposes the ones its body binds.
        let (antecedent_vars, columns): (Vec<FlirVar>, Vec<FlirVar>) = {
            let antecedent = bound_vars(&body);
            let helper = bound_vars(&helper_atoms);
            let antecedent_vars: Vec<FlirVar> = rule_scope
                .query_vars()
                .filter(|var| antecedent.contains(var.id()))
                .collect();
            let columns = antecedent_vars
                .iter()
                .filter(|var| helper.contains(var.id()))
                .cloned()
                .collect();
            (antecedent_vars, columns)
        };

        let output_schema = TableSchema::new(
            EntityRef::from(&rule.path),
            antecedent_vars.iter().cloned().map(Column::from).collect(),
            vec![],
        );
        rule_scope.constraints.insert(
            rule.path.clone(),
            ConstraintMeta::new(rule.rule.rule_variant, output_schema),
        );
        let head = FlirAtom::new_head(
            rule.path.clone(),
            antecedent_vars
                .into_iter()
                .map(frontend::Bind::Var)
                .collect(),
        );

        let path = ir::Path::from(format!("{}#consequent", rule.path));
        rule_scope.consequents.insert(
            path.clone(),
            columns.iter().cloned().map(Column::from).collect(),
        );
        let fields = || columns.iter().cloned().map(frontend::Bind::Var);
        let consequent = FlirRule {
            id: path.clone(),
            head: FlirAtom::new_head(path.clone(), fields().collect()),
            body: helper_atoms,
            conditions: helper_conditions,
        };
        body.push(FlirAtom {
            name: path,
            negated: true,
            bindings: fields().enumerate().collect(),
        });
        Ok(Self {
            rule,
            head,
            body,
            conditions,
            consequent,
        })
    }

    /// The constraint's own rule, plus the helper rule deriving its consequent.
    fn into_rules(self) -> [FlirRule; 2] {
        let rule = FlirRule {
            id: self.rule.path.clone(),
            head: self.head,
            body: self.body,
            conditions: self.conditions,
        };
        [rule, self.consequent]
    }
}

/// If `atoms` and `conditions` stand as a rule body of their own: there is an
/// atom to read from, and the atoms bind every variable the conditions use.
fn stands_alone(atoms: &[FlirAtom], conditions: &[FlirCond]) -> bool {
    let bound = bound_vars(atoms);
    !atoms.is_empty()
        && conditions
            .iter()
            .flat_map(FlirCond::vars)
            .all(|var| bound.contains(var.id()))
}

/// The names of the variables `atoms` bind.
fn bound_vars(atoms: &[FlirAtom]) -> HashSet<&ir::Path> {
    atoms
        .iter()
        .flat_map(frontend::Atom::vars)
        .map(Identifiable::id)
        .collect()
}

impl Identifiable for FlirConstraint<'_> {
    type Identifier = ir::Path;
    fn id(&self) -> &Self::Identifier {
        &self.rule.path
    }
}

#[derive(Debug, Clone)]
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
    fn from_atom(atom: &ir::Atom, resolver: &RuleScopeGuard) -> Result<FlirAtom, SyntaxError> {
        let mut bindings = Vec::with_capacity(
            if atom.row_id.is_some() {
                StoreEngineCols::ROW_ID_COLS
            } else {
                0
            } + atom.values.len(),
        );
        let entity = &atom.entity;
        let schema = resolver.schema(entity).ok_or_else(|| {
            SyntaxError::new(format!("atom over '{entity}' has no schema declared"))
        })?;
        let mut bind = |idx: CompilerColIdx, el: &ir::El| -> Result<(), SyntaxError> {
            // The index comes from the query schema but the name comes from the variable.
            let query_cols = schema.resolve_query_cols(idx).ok_or_else(|| {
                SyntaxError::new(format!(
                    "atom over '{entity}' binds compiler column {idx:?} \
                     but its schema does not resolve it"
                ))
            })?;
            let vars = resolve_element(el, resolver);
            if query_cols.len() != vars.len() {
                return Err(SyntaxError::new(format!(
                    "atom over '{entity}' binds {} term(s) to compiler column {idx:?}, \
                     which spans {} query column(s)",
                    vars.len(),
                    query_cols.len(),
                )));
            }
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
            negated: false,
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

/// A rule still being built from the FLIR, and hence named but not yet
/// printable as Datalog.
fn rule_frame(path: &ir::Path) -> Frame {
    Frame::Rule {
        name: path.to_string(),
        text: None,
    }
}

fn resolve_propositions(
    propositions: &[ir::Prop],
    resolver: &RuleScopeGuard,
) -> Result<(Vec<FlirAtom>, Vec<FlirCond>), SyntaxError> {
    let (body, conditions): (Vec<FlirAtom>, Vec<FlirCond>) = propositions.iter().try_fold(
        (Vec::new(), Vec::new()),
        |(mut body, mut conditions), prop| {
            match prop {
                ir::Prop::Atom { atom } => body.push(FlirAtom::from_atom(atom, resolver)?),
                ir::Prop::Eq { equality } => conditions.extend(FlirCond::new(
                    Operator::Equal,
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

#[derive(Debug, Clone)]
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

    /// The variables the condition reads, on either side.
    fn vars(&self) -> impl Iterator<Item = &FlirVar> {
        [&self.left, &self.right]
            .into_iter()
            .filter_map(|bind| match bind {
                frontend::Bind::Var(var) => Some(var),
                frontend::Bind::Lit(_) => None,
            })
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

    /// Lowers `file_name` step by step, rather than by
    /// [`FlirProgram::from_flat_realm`], such that the reason is shown:
    /// a rejected program's quotient graph is part of the panic message along
    /// with its validation error (e.g. non-stratifiable).
    fn query_program_from_json_flir(file_name: &str) -> FlirProgram {
        let logical = logical_program_from_json_flir(file_name);
        let verified = logical.verify().unwrap_or_else(|rejected| {
            panic!(
                "{file_name} an invalid logical program: {}\n{}",
                rejected.error,
                rejected.analysis.dot(frontend::Columns::Hidden)
            )
        });
        let code = logical
            .prepare(verified)
            .unwrap_or_else(|err| panic!("{file_name} cannot be translated into query ir: {err}"));
        FlirProgram::new(logical, code)
    }

    fn logical_program_from_json_flir(file_name: &str) -> FlirLogicalProgram {
        let flat_realm = coln_flir_rs::test_utils::load_theory_from_json(file_name);
        FlirLogicalProgram::from_flat_realm(&flat_realm).unwrap_or_else(|err| {
            panic!("{file_name} can be JSON-parsed but no (unverified) logical program can be made of it: {err}")
        })
    }

    #[test]
    fn graph_flir() -> Result<(), SyntaxError> {
        let logical = logical_program_from_json_flir("GraphRealm.json");
        println!("{:#}", logical.display());
        let analysis = logical.analyze();
        println!("{:#}", analysis.dot(frontend::Columns::Hidden));
        println!("{:#}", analysis.dot(frontend::Columns::Shown));
        let code = logical.prepare(analysis.verify().expect("program is valid"))?;
        let program = FlirProgram::new(logical, code);
        println!("{}", program.to_tree());
        Ok(())
    }

    #[test]
    fn graph_of_graphs_flir() -> Result<(), SyntaxError> {
        let logical = logical_program_from_json_flir("GraphOfGraphsRealm.json");
        println!("{:#}", logical.display());
        let analysis = logical.analyze();
        println!("{:#}", analysis.dot(frontend::Columns::Hidden));
        println!("{:#}", analysis.dot(frontend::Columns::Shown));
        let code = logical.prepare(analysis.verify().expect("program is valid"))?;
        let program = FlirProgram::new(logical, code);
        println!("{}", program.to_tree());
        Ok(())
    }

    #[test]
    fn triangle_flir() {
        let program = query_program_from_json_flir("TriangleRealm.json");
        println!("{:#}", program.logical().display());
        println!("{}", program.to_tree());
    }

    #[test]
    fn transitive_closure_flir() -> Result<(), SyntaxError> {
        let logical = logical_program_from_json_flir("TransitiveClosureRealm.json");
        println!("{:#}", logical.display());
        let analysis = logical.analyze();
        println!("{:#}", analysis.dot(frontend::Columns::Hidden));
        println!("{:#}", analysis.dot(frontend::Columns::Shown));
        let code = logical.prepare(analysis.verify().expect("program is valid"))?;
        let program = FlirProgram::new(logical, code);
        println!("{}", program.to_tree());
        Ok(())
    }

    /// Expected to panic until a compiler bug is fixed: the compiler emits an
    /// atom binding the row id of the derived view `init.trans-closure.connected`
    /// (in rule `init.trans-closure.snoc.collect`), but derived views carry no row id.
    /// Once the compiler is fixed, this test fails and drop the `should_panic`.
    #[test]
    // #[should_panic(expected = "binds compiler column RowId")]
    fn transitive_closure_set_flir() {
        let program = query_program_from_json_flir("TransitiveClosureSetRealm.json");
        println!("{:#}", program.logical().display());
        println!("{}", program.to_tree());
    }
}
