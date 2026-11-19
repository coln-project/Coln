// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module converts coln's flattened lowered intermediate representation
//! (FLIR) into a [logical program (Datalog)](LogicalProgram) and into
//! [QueryIr] which can eventually be executed by the query engine(s).

use crate::error::{Frame, SyntaxError};
use crate::frontend::{self, Identifiable, LogicalProgram, Prepared, Proposition};
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
use std::collections::HashSet;

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

        let law_rules: Vec<frontend::Rule<Flir>> = flat_realm
            .rules
            .iter()
            .filter(|law| !law.rule.consequents.is_empty())
            .map(|law| {
                FlirLaw::new(law, &mut ctx)
                    .map(FlirLaw::into_rules)
                    .map_err(|error| error.within(rule_frame(&law.path)))
            })
            .collect::<Result<Vec<_>, SyntaxError>>()?
            .into_iter()
            .flatten()
            .collect();

        let mut rules = ground_table_rules;
        rules.extend(derived_view_rules);
        rules.extend(law_rules);

        let predicates =
            frontend::Predicate::group(ctx.declarations(), rules).map_err(|unmatched| {
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
    /// The helper predicates deriving the laws' consequents, with their
    /// columns. See [`FlirLaw`].
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
    fn declarations(&self) -> impl Iterator<Item = (ir::Path, Vec<Column>)> {
        let ground_table_declarations = self
            .ground_tables
            .iter()
            .map(|(path, meta)| (path, &meta.output_schema));
        let derived_view_declarations = self
            .derived_views
            .iter()
            .map(|(path, meta)| (path, &meta.output_schema));
        let law_declarations = self
            .laws
            .iter()
            .map(|(path, meta)| (path, &meta.output_schema));
        let consequent_declarations = self
            .consequents
            .iter()
            .map(|(path, columns)| (path.clone(), columns.clone()));
        ground_table_declarations
            .chain(derived_view_declarations.chain(law_declarations))
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
}

/// A proposition of a FLIR rule's body.
type FlirProposition = Proposition<frontend::Atom<Flir>, frontend::Cond<Flir>>;

impl From<FlirGroundTable<'_>> for frontend::Rule<Flir> {
    fn from(ground_table: FlirGroundTable<'_>) -> Self {
        Self {
            id: ground_table.id().clone(),
            head: ground_table.head,
            body: Vec::new(),
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
    body: Vec<FlirProposition>,
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

/// A law `antecedent ⇒ consequent`, as the rules deriving its violations:
/// the bindings of the antecedent for which the consequent does not hold.
///
/// The consequent is negated as a whole, `¬(c₁ ∧ … ∧ cₙ)`, which no single rule
/// body can express: negating each proposition on its own is `¬c₁ ∧ … ∧ ¬cₙ`.
/// So the consequent becomes a helper predicate of its own, named
/// `<rule>#consequent`, and the law's rule negates that one atom:
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
/// The helper's body is the given consequent alone if it stands as a rule on
/// its own, that is, its atoms bind every variable its conditions use.
/// Otherwise, as for `∀x. t(x) ⇒ x == 1`, the antecedent joins the consquent
/// rule's body. That changes nothing, as the law's rule requires the
/// antecedent anyway, but costs computing it twice, which is why it is not
/// done if not required.
#[derive(Debug)]
struct FlirLaw<'a> {
    rule: &'a ir::RuleEntry,
    head: frontend::Atom<Flir>,
    body: Vec<FlirProposition>,
    /// The helper rule deriving the consequent.
    consequent: frontend::Rule<Flir>,
}

impl<'a> FlirLaw<'a> {
    fn new(rule: &'a ir::RuleEntry, ctx: &mut FlirContext) -> Result<Self, SyntaxError> {
        let mut rule_scope = ctx.enter_rule_scope(&rule.rule.vars);
        let mut body = resolve_propositions(&rule.rule.antecedents, &rule_scope)?;
        let consequent = resolve_propositions(&rule.rule.consequents, &rule_scope)?;

        // TODO: Potential optimization with a single conjunct by not creating
        // a helper predicate but negating the single conjunct. Watch out though,
        // as this optimization is incompatible with empty antecedent rules
        // because then negation safety is violated, as any existential variable
        // in the consequent does not appear positively in any of the (empty)
        // antecedent's (positive) atoms.
        let helper_body = match stands_alone(&consequent) {
            true => consequent,
            false => body.iter().cloned().chain(consequent).collect(),
        };
        // A violation is a binding of the antecedent's variables, so those are
        // the law's columns: a variable only the consequent binds is
        // existential and has no value a violation could carry. Of them, the
        // helper exposes the ones its body binds.
        let (antecedent_vars, columns): (Vec<FlirVar>, Vec<FlirVar>) = {
            let antecedent = bound_vars(&body);
            let helper = bound_vars(&helper_body);
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
        rule_scope.laws.insert(
            rule.path.clone(),
            LawMeta::new(rule.rule.rule_variant, output_schema),
        );
        let head = frontend::Atom::new_head(
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
        let consequent = frontend::Rule {
            id: path.clone(),
            head: frontend::Atom::new_head(path.clone(), fields().collect()),
            body: helper_body,
            meta: (),
        };
        body.push(Proposition::Atom(frontend::Atom {
            name: path,
            negated: true,
            bindings: fields().enumerate().collect(),
        }));
        Ok(Self {
            rule,
            head,
            body,
            consequent,
        })
    }

    /// The law's own rule, plus the helper rule deriving its consequent.
    fn into_rules(self) -> [frontend::Rule<Flir>; 2] {
        let rule = frontend::Rule {
            id: self.rule.path.clone(),
            head: self.head,
            body: self.body,
            meta: (),
        };
        [rule, self.consequent]
    }
}

/// If `body` stands as a rule body of its own:
/// All its conditions' variables are bound by some of its atoms.
fn stands_alone(body: &[FlirProposition]) -> bool {
    let bound = bound_vars(body);
    body.iter()
        .filter_map(|prop| prop.as_ref().cond())
        .flat_map(frontend::Cond::vars)
        .all(|var| bound.contains(var.id()))
}

/// The names of the variables the atoms of `body` bind.
fn bound_vars(body: &[FlirProposition]) -> HashSet<&ir::Path> {
    body.iter()
        .filter_map(|prop| prop.as_ref().atom())
        .flat_map(frontend::Atom::vars)
        .map(Identifiable::id)
        .collect()
}

impl Identifiable for FlirLaw<'_> {
    type Identifier = ir::Path;
    fn id(&self) -> &Self::Identifier {
        &self.rule.path
    }
}

impl frontend::Atom<Flir> {
    fn new_head(name: ir::Path, fields: Vec<frontend::Bind<FlirVar, FlirLit>>) -> Self {
        Self {
            name,
            negated: false,
            bindings: fields.into_iter().enumerate().collect(),
        }
    }
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
        Ok(Self {
            name: entity.clone(),
            negated: false,
            bindings,
        })
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
) -> Result<Vec<FlirProposition>, SyntaxError> {
    let resolved = propositions
        .iter()
        .map(|prop| resolve_proposition(prop, resolver))
        .collect::<Result<Vec<_>, SyntaxError>>()?;
    Ok(resolved.into_iter().flatten().collect())
}

/// One proposition of the FLIR, which an equality on row ids turns into two
/// conditions. See [`frontend::Cond::new`].
fn resolve_proposition(
    prop: &ir::Prop,
    resolver: &RuleScopeGuard,
) -> Result<MaybePair<FlirProposition>, SyntaxError> {
    Ok(match prop {
        ir::Prop::Atom { atom } => MaybePair::single(Proposition::Atom(frontend::Atom::from_atom(
            atom, resolver,
        )?)),
        ir::Prop::Eq { equality } => {
            frontend::Cond::new(Operator::Equal, &equality.left, &equality.right, resolver)
                .map(Proposition::Cond)
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
