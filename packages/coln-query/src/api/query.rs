// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module converts coln's flattened lowered intermediate representation
//! (FLIR) into a [logical program (Datalog)](LogicalProgram) and into
//! [QueryIr] which can eventually be executed by the query engine(s).

use crate::error::{Frame, SyntaxError};
use crate::frontend::{self, Identifiable, LogicalProgram, Prepared, Prop};
use crate::host::{QueryIr, expr, operator::Operator};
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
        ColnSchema, ColnSchemaWrapper, CompilerColIdx, DerivedView, GroundTable, NativeScalarType,
        QueryEngineCol, QueryEngineScalarType, ResolveCompilerIdxToQueryView, RuleVars,
        StoreEngineCols,
    },
};
use indexmap::IndexMap;
use std::borrow::Cow;

type GroundTableName = EntityRef;
type DerivedViewName = EntityRef;
type LawName = EntityRef;

/// Coln's FLIR frontend's [`QueryProgram`]: what a [`FlatRealm`] lowers to,
/// ready to run.
///
/// The only way to one is [`new`](Self::new), which takes [`Prepared`] code
/// only, so a `FlirProgram`'s code always stems from a verified program. For a
/// program that may be invalid, stop at its [`FlirLogicalProgram`].
///
/// The [`Catalog`] half is served straight out of the logical program's
/// [`ground_tables`](FlirLogicalProgram::ground_tables), which stores FLIR's own
/// richer [`ColnSchema<GroundTable>`](ColnSchema) which includes the schema
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
    predicates: Vec<frontend::Predicate<Flir>>,
    /// The declared ground tables. Doubles as the [`FlirProgram`]'s [`Catalog`]: every
    /// [`SourceExpr`](crate::relational::expr::SourceExpr) the lowering mints
    /// names one of these.
    ground_tables: IndexMap<GroundTableName, PredicateMeta<GroundTable>>,
    /// Any materialized, maintained, derived view. Doubles as this program's
    /// [`Catalog`] but for adhoc-queries, which are allowed to read from the
    /// materialized views, too, as opposed to the incrementally-maintained
    /// queries defined in here.
    ///
    /// This doubles as the set of derived views an [`Atom`](ir::Atom) may
    /// reference, and as what [`derived_view_meta`](Self::derived_view_meta)
    /// tells a derived view's sink apart by.
    derived_views: IndexMap<DerivedViewName, PredicateMeta<DerivedView>>,
    /// The laws the program defines, one per declared law, each of which is
    /// either enforced or monitored.
    laws: IndexMap<LawName, LawMeta>,
}

#[derive(Debug, Clone)]
pub struct PredicateMeta<Marker> {
    coln_schema: ColnSchema<Marker>,
    output_schema: TableSchema,
}

#[derive(Debug)]
pub struct LawMeta {
    kind: ir::RuleVariant,
    output_schema: TableSchema,
}

impl LawMeta {
    fn new(kind: ir::RuleVariant, output_schema: TableSchema) -> Self {
        LawMeta {
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
/// The implicit row id leads the list of keys: it is the only key a ground table
/// is guaranteed to have and to be unique on, so a backend that can index by
/// just one key (DBSP) picks it by taking the first.
impl From<&ColnSchema<GroundTable>> for TableSchema {
    fn from(value: &ColnSchema<GroundTable>) -> Self {
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
        let verified = logical.verify().map_err(|rejected| rejected.error)?;
        let code = logical.prepare(verified);
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

        let ground_table_rules: Vec<frontend::Rule<Flir>> = flat_realm
            .tables
            .iter()
            .flat_map(|declaration| {
                let ground_table = ctx.feed_table_declaration(declaration)?;
                Some(frontend::Rule::from(ground_table))
            })
            .collect();

        let derived_view_rules: Vec<frontend::Rule<Flir>> = flat_realm
            .definitions
            .iter()
            .map(|definition| {
                let rule = FlirDerivedViewRule::new(definition, &mut ctx)
                    .map_err(|error| error.within(rule_frame(&definition.path)))?;
                Ok(frontend::Rule::from(rule))
            })
            .collect::<Result<_, SyntaxError>>()?;

        let law_and_helper_predicates = flat_realm
            .rules
            .iter()
            .filter(|law| !law.rule.consequents.is_empty())
            .map(|law| {
                ctx.feed_law(law)
                    .map_err(|error| error.within(rule_frame(&law.path)))
            })
            .collect::<Result<Vec<_>, SyntaxError>>()?
            .into_iter()
            .flat_map(|(law_predicate, helper_predicate)| {
                std::iter::once(law_predicate).chain(helper_predicate)
            });

        let mut rules = ground_table_rules;
        rules.extend(derived_view_rules);

        let mut predicates =
            frontend::Predicate::group(ctx.declarations(), rules).map_err(|unmatched| {
                SyntaxError::new(format!("Unmatched rules with no definand {unmatched}"))
            })?;
        // A law's predicates come defined by their single rule each, so they
        // need no grouping.
        predicates.extend(law_and_helper_predicates);

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
            ground_tables: remap(ctx.ground_tables),
            derived_views: remap(ctx.derived_views),
            laws: remap(ctx.laws),
        })
    }

    pub fn law_meta(&self, sink: &SinkId) -> Option<&LawMeta> {
        self.laws.get(&LawName::from(sink))
    }

    pub fn derived_view_meta(&self, sink: &SinkId) -> Option<&PredicateMeta<DerivedView>> {
        self.derived_views.get(&DerivedViewName::from(sink))
    }
}

impl Catalog for FlirProgram {
    /// Projects FLIR's [`ColnSchema<GroundTable>`](ColnSchema) down to the
    /// [`TableSchema`] a plan needs, on demand. [`Cow::Owned`] rather than a
    /// borrow precisely so that the richer schema stays the only stored copy.
    ///
    /// Only ground tables answer here: a rule's output is bound to a host
    /// variable and referenced by [`VarExpr`](expr::VarExpr), never by a
    /// [`SourceExpr`](crate::relational::expr::SourceExpr), so
    /// [`derived_views`](FlirLogicalProgram::derived_views) is no part of the
    /// catalog.
    fn source_schema(&self, id: &SourceId) -> Option<Cow<'_, TableSchema>> {
        self.logical
            .ground_tables
            .get(&GroundTableName::from(id))
            .map(|meta| Cow::Borrowed(&meta.output_schema))
    }
}

impl LogicalProgram for FlirLogicalProgram {
    type Lang = Flir;

    fn predicates(&self) -> impl Iterator<Item = &frontend::Predicate<Flir>> {
        self.predicates.iter()
    }

    /// Exactly the derived views and the laws, that is, what
    /// [`interpret_outputs`](super::ColnQuery) knows how to report. A helper
    /// predicate, such as a law's consequent, is neither.
    fn is_output(&self, predicate: &frontend::Predicate<Flir>) -> bool {
        let entity = EntityRef::from(predicate.id());
        self.derived_views.contains_key(&entity) || self.laws.contains_key(&entity)
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
    ground_tables: IndexMap<ir::Path, PredicateMeta<GroundTable>>,
    derived_views: IndexMap<ir::Path, PredicateMeta<DerivedView>>,
    laws: IndexMap<ir::Path, LawMeta>,
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
        self.ground_tables
            .get(entity)
            .map(|meta| ColnSchemaWrapper::GroundTable(&meta.coln_schema))
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
    ) -> Option<FlirGroundTable<'a>> {
        let entity = &declaration.path;
        if let Some(ground_table) = FlirGroundTable::new(declaration, self) {
            return Some(ground_table);
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
            "Table declaration for '{entity}' neither valid as a ground table nor as a derived view"
        )
    }
    /// Lowers `law` into the predicate deriving its violations and, unless
    /// inlined, the helper predicate deriving its consequent, see
    /// [`frontend::Law`], and registers its [`LawMeta`].
    fn feed_law(
        &mut self,
        law: &ir::RuleEntry,
    ) -> Result<(frontend::Predicate<Flir>, Option<frontend::Predicate<Flir>>), SyntaxError> {
        let mut rule_scope = self.enter_rule_scope(&law.rule.vars);
        let (violations, consequent) = frontend::Law::new(
            law.path.clone(),
            law.path.clone(),
            rule_scope.query_vars().collect(),
            resolve_propositions(&law.rule.antecedents, &rule_scope)?,
            resolve_propositions(&law.rule.consequents, &rule_scope)?,
            (),
        )
        .lower();
        let output_schema = TableSchema::new(
            EntityRef::from(&law.path),
            violations.columns().cloned().collect(),
            vec![],
        );
        rule_scope.laws.insert(
            law.path.clone(),
            LawMeta::new(law.rule.rule_variant, output_schema),
        );
        Ok((violations, consequent))
    }
    /// The declarations of the ground tables and the derived views. A law's
    /// predicates come defined already, see [`Self::feed_law`].
    fn declarations(&self) -> impl Iterator<Item = (ir::Path, Vec<Column>)> {
        let ground_table_declarations = self
            .ground_tables
            .iter()
            .map(|(path, meta)| (path, &meta.output_schema));
        let derived_view_declarations = self
            .derived_views
            .iter()
            .map(|(path, meta)| (path, &meta.output_schema));
        ground_table_declarations
            .chain(derived_view_declarations)
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

/// FLIR's [`frontend::Language`]: a path names a predicate, a rule, or a
/// variable alike.
#[derive(Debug, Clone)]
pub struct Flir;

impl frontend::Language for Flir {
    type PredId = ir::Path;
    type RuleId = ir::Path;
    type Var = FlirVar;
    type Lit = FlirLit;
    type RuleMeta = ();

    /// `<base>#<tag>`. For now we assume coln-compiler never puts a `#` into a
    /// path, so no path of the source is derived, and a derivation can be told
    /// apart from its base.
    fn derive_pred_id(base: &ir::Path, tag: &'static str) -> ir::Path {
        ir::Path::from(format!("{base}#{tag}"))
    }

    /// `<base>#<tag>`, as for
    /// [`derive_pred_id`](frontend::Language::derive_pred_id).
    fn derive_rule_id(base: &ir::Path, tag: &'static str) -> ir::Path {
        ir::Path::from(format!("{base}#{tag}"))
    }
}

impl From<FlirGroundTable<'_>> for frontend::Rule<Flir> {
    fn from(ground_table: FlirGroundTable<'_>) -> Self {
        Self {
            id: ground_table.id().clone(),
            head: ground_table.head,
            body: frontend::Conjunction::default(),
            meta: (),
        }
    }
}

impl From<FlirDerivedViewRule<'_>> for frontend::Rule<Flir> {
    fn from(rule: FlirDerivedViewRule<'_>) -> Self {
        Self {
            id: rule.id().clone(),
            head: rule.head,
            body: rule.body,
            meta: (),
        }
    }
}

#[derive(Debug)]
struct FlirGroundTable<'a> {
    ground_table: &'a ir::TableEntry,
    head: frontend::Atom<Flir>,
}

impl<'a> FlirGroundTable<'a> {
    fn new(ground_table: &'a ir::TableEntry, ctx: &mut FlirContext) -> Option<FlirGroundTable<'a>> {
        let schema = Option::<ColnSchema<GroundTable>>::from(ground_table)?;
        let head = frontend::Atom::new_head(
            ground_table.path.clone(),
            schema
                .query_cols()
                .inner()
                .iter()
                .map(|col| frontend::Bind::Var(FlirVar::from(col)))
                .collect(),
        );
        ctx.ground_tables.insert(
            ground_table.path.clone(),
            PredicateMeta {
                output_schema: TableSchema::from(&schema),
                coln_schema: schema,
            },
        );
        Some(Self { ground_table, head })
    }
}

impl Identifiable for FlirGroundTable<'_> {
    type Identifier = ir::Path;
    fn id(&self) -> &Self::Identifier {
        &self.ground_table.path
    }
}

/// One rule defining a derived view. A derived view may be defined by several
/// such rules, all sharing the view as their head's predicate.
#[derive(Debug)]
struct FlirDerivedViewRule<'a> {
    rule: &'a ir::DefinitionEntry,
    head: frontend::Atom<Flir>,
    body: frontend::Conjunction<Flir>,
}

impl<'a> FlirDerivedViewRule<'a> {
    /// Returns `Err` if the [rule's definand](ir::Definition::definand) has
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
        let head = frontend::Atom::new_head(
            definand.clone(),
            rule.definition
                .arguments
                .iter()
                .flat_map(|argument| resolve_element(argument, &rule_scope))
                .collect(),
        );
        let body = resolve_propositions(&rule.definition.antecedents, &rule_scope)?;
        Ok(FlirDerivedViewRule { rule, head, body })
    }
}

impl Identifiable for FlirDerivedViewRule<'_> {
    type Identifier = ir::Path;
    fn id(&self) -> &Self::Identifier {
        &self.rule.path
    }
}

impl frontend::Atom<Flir> {
    fn from_atom(atom: &ir::Atom, resolver: &RuleScopeGuard) -> Result<Self, SyntaxError> {
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
        Ok(Self::new_positive(entity.clone(), bindings))
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
) -> Result<frontend::Conjunction<Flir>, SyntaxError> {
    let resolved = propositions
        .iter()
        .map(|prop| resolve_proposition(prop, resolver))
        .collect::<Result<Vec<_>, SyntaxError>>()?;
    Ok(resolved.into_iter().flatten().collect())
}

type FlirProp = Prop<frontend::Atom<Flir>, frontend::Cond<Flir>>;

/// One proposition of the FLIR, which an equality on row ids turns into two
/// conditions. See [`frontend::Cond::new`].
fn resolve_proposition(
    prop: &ir::Prop,
    resolver: &RuleScopeGuard,
) -> Result<MaybePair<FlirProp>, SyntaxError> {
    Ok(match prop {
        ir::Prop::Atom { atom } => {
            MaybePair::single(Prop::Atom(frontend::Atom::from_atom(atom, resolver)?))
        }
        ir::Prop::Eq { equality } => {
            frontend::Cond::new(Operator::Equal, &equality.left, &equality.right, resolver)
                .map(Prop::Cond)
        }
    })
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
    fn map<U>(self, f: impl Fn(T) -> U) -> MaybePair<U> {
        MaybePair(f(self.0), self.1.map(f))
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

impl frontend::Cond<Flir> {
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
        expr::Literal::from(&self.inner)
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

impl From<&ir::Lit> for expr::Literal {
    fn from(value: &ir::Lit) -> Self {
        match value {
            ir::Lit::Int { value } => expr::Literal::Iint(*value as i64),
            ir::Lit::String { value } => expr::Literal::String(value.clone()),
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
    use crate::test_utils::dump::Dump;

    /// Lowers `file_name` step by step, rather than by
    /// [`FlirProgram::from_flat_realm`], dumping every stage on the way: the
    /// Datalog, the analysis drawn with and without columns, and the query IR.
    ///
    /// The graphs are dumped before verification, so a rejected program's are,
    /// too. Its panic message then only carries the validation error.
    fn query_program_from_json_flir(file_name: &str) -> FlirProgram {
        let dump = Dump::new(file_name.trim_end_matches(".json"));
        let logical = logical_program_from_json_flir(file_name);
        dump.write("program.dl", format_args!("{:#}", logical.display()));
        let analysis = logical.analyze();
        dump.write(
            "graph_noschema.dot",
            analysis.dot(frontend::Columns::Hidden),
        );
        dump.write("graph_schema.dot", analysis.dot(frontend::Columns::Shown));
        let verified = analysis.verify().unwrap_or_else(|rejected| {
            panic!(
                "{file_name} is an invalid logical program: {}",
                rejected.error
            )
        });
        let code = logical.prepare(verified);
        let program = FlirProgram::new(logical, code);
        dump.write("query.plan", program.to_tree());
        program
    }

    fn logical_program_from_json_flir(file_name: &str) -> FlirLogicalProgram {
        let flat_realm = coln_flir_rs::test_utils::load_theory_from_json(file_name);
        FlirLogicalProgram::from_flat_realm(&flat_realm).unwrap_or_else(|err| {
            panic!("{file_name} can be JSON-parsed but no (unverified) logical program can be made of it: {err}")
        })
    }

    /// This test is important because coln-store does some tests with an empty
    /// FLIR. Better to catch issues here early.
    #[test]
    fn empty_flir() -> Result<(), SyntaxError> {
        let flir = ir::FlatRealm {
            rules: Vec::new(),
            definitions: Vec::new(),
            tables: Vec::new(),
        };
        let program = FlirProgram::from_flat_realm(&flir)?;
        Ok(())
    }

    #[test]
    fn graph_flir() {
        query_program_from_json_flir("GraphRealm.json");
    }

    #[test]
    fn graph_of_graphs_flir() {
        query_program_from_json_flir("GraphOfGraphsRealm.json");
    }

    #[test]
    fn triangle_flir() {
        query_program_from_json_flir("TriangleRealm.json");
    }

    #[test]
    fn transitive_closure_flir() {
        query_program_from_json_flir("TransitiveClosureRealm.json");
    }

    #[test]
    fn bipartite_graph_flir() {
        query_program_from_json_flir("BipartiteGraphRealm.json");
    }

    #[test]
    fn bipartite_graph_over_flir() {
        query_program_from_json_flir("BipartiteGraphOverRealm.json");
    }

    /// Expected to panic until larger issue is resolved: Taking an initial
    /// model with a `Set` codomain requires giving up the clear EDB/IDB split,
    /// and interacting between them during query runtime.
    /// Once this is resolved, this test fails and drop the `should_panic`.
    #[test]
    #[should_panic(expected = "binds compiler column RowId")]
    fn transitive_closure_set_flir() {
        query_program_from_json_flir("TransitiveClosureSetRealm.json");
    }
}
