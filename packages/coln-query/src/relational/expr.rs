// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The relational-algebra plan layer.
//!
//! [`RelExpr`] is the backend-neutral query-plan operator vocabulary (relation →
//! relation). Every backend implements the [`RelExprVisitor`] family with its
//! own return type; the plan itself is shared. Relational operators are *also*
//! host expressions (via [`Expr::Relational`]), so operands stay [`Expr`], which
//! preserves relation-valued variables, nested operators, and tuple-of-relations.

use crate::{
    error::SyntaxError,
    host::{expr::Expr, stmt::BlockStmt},
    relational::{
        relation::{Tuple, TupleValue},
        schema::{EntityRef, TableSchema},
    },
    scalarial::{ScalarType, ScalarTypedValue},
};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU64;

/// Relational-algebra operator = backend-neutral query-plan vocabulary.
///
/// Operands stay [`Expr`] (a host expression that must evaluate to a relation),
/// which preserves relation-valued vars, nested ops, and tuple-of-relations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RelExpr {
    /// Leaf of the relational plan: names an extensional relation without
    /// carrying its runtime representation (no stream, no table). The backend
    /// binds the [`SourceId`] to a concrete relation at execution time.
    Source(Box<SourceExpr>),
    /// The other leaf of the relational plan: a relation the plan *carries*
    /// rather than names, as a fixed bag of rows. Nothing binds it.
    Constant(Box<ConstantExpr>),
    Output(Box<OutputExpr>),
    Alias(Box<AliasExpr>),
    Distinct(Box<DistinctExpr>),
    // A union can also be expressed with a full outer join and a projection.
    Union(Box<UnionExpr>),
    // As the antijoin is a generalization of the set difference, this may be
    // removed in the future.
    Difference(Box<DifferenceExpr>),
    Selection(Box<SelectionExpr>),
    Projection(Box<ProjectionExpr>),
    CartesianProduct(Box<CartesianProductExpr>),
    EquiJoin(Box<EquiJoinExpr>),
    MultiWayEquiJoin(Box<MultiWayEquiJoinExpr>),
    AntiJoin(Box<AntiJoinExpr>),
    FixedPointIter(Box<FixedPointIterExpr>),
}

/// Generates `From<XxxExpr> for RelExpr` (boxing into the given variant) and the
/// composed `From<XxxExpr> for Expr` (via [`Expr::Relational`]) so that
/// constructing a host expression from a relational operator stays a single
/// `Expr::from(..)`/`.into()` call, exactly as before the host/relational split.
///
/// Each also comes in a `Box<XxxExpr>` flavour, which reuses the allocation the
/// caller already holds. That is what an owned rewriting pass rebuilds an
/// untouched node with — see [`RelExprVisitorOwn`] — and since
/// [`Expr::Relational`] does not box what it wraps, that route allocates
/// nothing at all.
macro_rules! impl_rel_and_expr_from {
    ($(($variant:path, $expr:ty)),* $(,)?) => {
        $(
            impl From<$expr> for RelExpr {
                fn from(value: $expr) -> Self {
                    $variant(Box::new(value))
                }
            }
            impl From<Box<$expr>> for RelExpr {
                fn from(value: Box<$expr>) -> Self {
                    $variant(value)
                }
            }
            impl From<$expr> for Expr {
                fn from(value: $expr) -> Self {
                    Expr::Relational(RelExpr::from(value))
                }
            }
            impl From<Box<$expr>> for Expr {
                fn from(value: Box<$expr>) -> Self {
                    Expr::Relational(RelExpr::from(value))
                }
            }
        )*
    };
}

impl_rel_and_expr_from! {
    (RelExpr::Source, SourceExpr),
    (RelExpr::Constant, ConstantExpr),
    (RelExpr::Output, OutputExpr),
    (RelExpr::Alias, AliasExpr),
    (RelExpr::Distinct, DistinctExpr),
    (RelExpr::Union, UnionExpr),
    (RelExpr::Difference, DifferenceExpr),
    (RelExpr::Selection, SelectionExpr),
    (RelExpr::Projection, ProjectionExpr),
    (RelExpr::CartesianProduct, CartesianProductExpr),
    (RelExpr::EquiJoin, EquiJoinExpr),
    (RelExpr::MultiWayEquiJoin, MultiWayEquiJoinExpr),
    (RelExpr::AntiJoin, AntiJoinExpr),
    (RelExpr::FixedPointIter, FixedPointIterExpr),
}

/// The single bridge from the relational layer back into the host layer: a
/// relational operator is *also* a host expression. Free of charge, since
/// [`Expr::Relational`] does not box what it wraps.
impl From<RelExpr> for Expr {
    fn from(value: RelExpr) -> Self {
        Expr::Relational(value)
    }
}

/// [`RelExpr`] without the payloads: which *kind* of operator a node is.
///
/// This is the vocabulary a rewriting rule declares its interest in, so a
/// driver can skip offering it nodes it could never fire on — see
/// [`TransformationRule::interest`](crate::optimizer::rewrite::TransformationRule::interest).
/// A structural precondition of that shape is worth stating separately from the
/// rewrite itself; a *semantic* one is not, because checking it means taking
/// the node apart, which the rewrite then has to do again.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum RelKind {
    Source,
    Constant,
    Output,
    Alias,
    Distinct,
    Union,
    Difference,
    Selection,
    Projection,
    CartesianProduct,
    EquiJoin,
    MultiWayEquiJoin,
    AntiJoin,
    FixedPointIter,
}

impl RelKind {
    /// Every kind, for a rule that has to see the whole plan.
    pub const ALL: &'static [RelKind] = &[
        RelKind::Source,
        RelKind::Constant,
        RelKind::Output,
        RelKind::Alias,
        RelKind::Distinct,
        RelKind::Union,
        RelKind::Difference,
        RelKind::Selection,
        RelKind::Projection,
        RelKind::CartesianProduct,
        RelKind::EquiJoin,
        RelKind::MultiWayEquiJoin,
        RelKind::AntiJoin,
        RelKind::FixedPointIter,
    ];
}

impl RelExpr {
    /// Which operator this node is, without looking at its operands.
    pub fn kind(&self) -> RelKind {
        match self {
            RelExpr::Source(_) => RelKind::Source,
            RelExpr::Constant(_) => RelKind::Constant,
            RelExpr::Output(_) => RelKind::Output,
            RelExpr::Alias(_) => RelKind::Alias,
            RelExpr::Distinct(_) => RelKind::Distinct,
            RelExpr::Union(_) => RelKind::Union,
            RelExpr::Difference(_) => RelKind::Difference,
            RelExpr::Selection(_) => RelKind::Selection,
            RelExpr::Projection(_) => RelKind::Projection,
            RelExpr::CartesianProduct(_) => RelKind::CartesianProduct,
            RelExpr::EquiJoin(_) => RelKind::EquiJoin,
            RelExpr::MultiWayEquiJoin(_) => RelKind::MultiWayEquiJoin,
            RelExpr::AntiJoin(_) => RelKind::AntiJoin,
            RelExpr::FixedPointIter(_) => RelKind::FixedPointIter,
        }
    }
}

/// Backend-neutral identity of a relation source. The tree only ever *names* a
/// source; the backend maps it to a concrete relation (DBSP stream, batch Z-set,
/// or SQL table/view) at execution time.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SourceId(pub String);

impl SourceId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for SourceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for SourceId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for SourceId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

/// Sources are named in diagnostics and in rendered plans, so the id renders as
/// the bare name it is.
impl std::fmt::Display for SourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// So a `HashMap<SourceId, _>` can be probed with a `&str`, as one keyed by
/// `String` could. Without it every lookup would have to mint an owned
/// [`SourceId`] first.
impl std::borrow::Borrow<str> for SourceId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// Backend-neutral relation leaf: it *names* an extensional relation and carries
/// nothing else — no schema, no stream, no table. What the name means is answered
/// by the [`Catalog`](crate::relational::catalog::Catalog) the plan is compiled
/// against, so a relation the plan references `N` times is described once instead
/// of `N` times.
///
/// Naming rather than describing is also what makes the derived [`PartialEq`]
/// mean what it reads as. A backend's physical schema
/// (say a [`StreamSchema`](crate::relational::incremental::schema::StreamSchema))
/// compares only its key and tuple, deliberately ignoring its `name` (a
/// transformation trace rather than an identity) — so back when this leaf held a
/// schema, two leaves naming *different* relations compared equal whenever their
/// shapes matched.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceExpr {
    pub id: SourceId,
}

impl SourceExpr {
    /// Build a source leaf naming an extensional input. The backend binds the
    /// [`SourceId`] to a concrete relation at execution time, and the plan's
    /// [`Catalog`](crate::relational::catalog::Catalog) answers what it is.
    pub fn new(id: impl Into<SourceId>) -> Self {
        Self { id: id.into() }
    }

    pub fn as_id(&self) -> &SourceId {
        &self.id
    }
}

/// How many copies of a row a [`ConstantExpr`] holds.
///
/// Unsigned and non-zero, unlike a
/// [`ZWeight`](crate::relational::incremental::dbsp::ZWeight).
/// A relation the plan *states* cannot hold a negative number of copies of a
/// row, and zero copies is the absence of a row rather than an entry about it.
/// Keeping the zweight out of the plan layer is what makes both unrepresentable
/// instead of merely wrong.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Multiplicity(NonZeroU64);

impl Multiplicity {
    /// A row present exactly once, which is what set-shaped data means.
    pub const ONE: Self = Self(NonZeroU64::MIN);

    /// [`None`] for zero copies; see the type's docs.
    pub fn new(copies: u64) -> Option<Self> {
        NonZeroU64::new(copies).map(Self)
    }

    pub fn get(self) -> u64 {
        self.0.get()
    }

    /// Fold two entries for the same row into one. [`None`] on overflow, which
    /// [`ConstantExpr::new`] reports rather than wrapping around.
    fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.get()).map(Self)
    }
}

impl std::fmt::Display for Multiplicity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "×{}", self.0)
    }
}

/// The name [`ConstantExpr::unit`] labels its schema with. A constant is not
/// bound by name, so this is a label for diagnostics rather than an identity.
const UNIT_NAME: &str = "unit";

/// How many rows a rendered constant shows before eliding the rest: enough to
/// recognise which relation it is, few enough that a plan dump stays readable.
const RENDERED_ROWS: usize = 8;

/// A relation the plan carries: a fixed bag of rows, fully determined before
/// execution. The second kind of plan leaf and [`SourceExpr`]'s counterpart —
/// that one names rows a driver feeds, this one holds them, so nothing binds it
/// and no [`Catalog`](crate::relational::catalog::Catalog) describes it.
///
/// **What "static" means for incremental computations.** It is a claim about
/// time: the rows are there from the first commit on and are never retracted.
/// A backend therefore emits them as *one* delta, and the integrated value is
/// [`rows`](Self::rows) at every step. That is also what makes a constant
/// loop-invariant inside a [`FixedPointIterExpr`] step, where it enters the
/// iteration exactly like an outer relation does.
///
/// **A bag, not a set.** A row may be present several times according to its
/// [`Multiplicity`]. Multiplicity is observable only through operators
/// which count, e.g. a join multiplies the weights of its operands, so joining
/// against a row held twice doubles the other side. A [`DistinctExpr`]
/// collapses duplicates into one occurrence.
///
/// **Canonical form.** [`new`](Self::new) folds equal rows into one entry and
/// orders them, so the derived [`PartialEq`] is bag equality rather than
/// "written in the same order". That is what lets a rewrite rule recognise two
/// occurrences of one constant as the same relation, and a plan assertion in a
/// test not depend on the order the rows were listed in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstantExpr {
    /// What the rows *are*: named, typed columns plus the key(s) over them. The
    /// same vocabulary a [`Catalog`](crate::relational::catalog::Catalog)
    /// answers with for a [`SourceExpr`], so everything downstream, the type
    /// resolver, each backend's physical schema, reuses the source path instead
    /// of growing a second notion of what a relation looks like.
    ///
    /// Declared rather than inferred from the rows: the arity of a constant with
    /// no rows is unrecoverable, and a projection above it needs names and types
    /// either way. Its [`name`](TableSchema::name) is a diagnostic label, not an
    /// identity anything binds against.
    schema: TableSchema,
    /// The rows and how many copies of each, in [canonical form](ConstantExpr).
    rows: Vec<(TupleValue, Multiplicity)>,
}

impl ConstantExpr {
    /// The _only_ constructor and it cannot produce a malformed constant, as
    /// it canonicalizes the rows and then applies [`validate`](Self::validate).
    pub fn new(
        schema: TableSchema,
        rows: impl IntoIterator<Item = (TupleValue, Multiplicity)>,
    ) -> Result<Self, SyntaxError> {
        let mut constant = Self {
            schema,
            rows: rows.into_iter().collect(),
        };
        constant.canonicalize()?;
        constant.validate()?;
        Ok(constant)
    }

    /// Set-shaped data: every listed row present once. Listing a row twice is
    /// not an error — it is a bag holding two copies of it, see
    /// [`Multiplicity`].
    pub fn set(
        schema: TableSchema,
        rows: impl IntoIterator<Item = TupleValue>,
    ) -> Result<Self, SyntaxError> {
        Self::new(schema, rows.into_iter().map(|row| (row, Multiplicity::ONE)))
    }

    /// The relation holding exactly the *unit tuple*: no columns, one 0-tuple.
    /// The neutral element of the join, which is what a rule with an empty
    /// antecedent needs its body to be.
    ///
    /// Valid by construction: No columns to disagree with and no declared key
    /// to violate. Hence, this skips [`validate`](Self::validate).
    pub fn unit() -> Self {
        Self {
            schema: TableSchema::new(EntityRef::from(UNIT_NAME), vec![], vec![]),
            rows: vec![(TupleValue::empty(), Multiplicity::ONE)],
        }
    }

    /// No rows at all over `schema`: the neutral element of the union. Valid by
    /// construction, as [`unit`](Self::unit) is.
    pub fn empty(schema: TableSchema) -> Self {
        Self {
            schema,
            rows: Vec::new(),
        }
    }

    pub fn schema(&self) -> &TableSchema {
        &self.schema
    }

    /// The rows, canonical: each distinct row once, with its multiplicity,
    /// ordered.
    pub fn rows(&self) -> &[(TupleValue, Multiplicity)] {
        &self.rows
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Whether this is [`unit`](Self::unit) _with a single copy_ of the 0-tuple:
    /// the join's neutral element, and so the one shape a join-elimination rule
    /// may drop. At multiplicity `m` the join would scale its other operand by
    /// `m` instead, which is why the test is not just "no columns, one row".
    pub fn is_unit(&self) -> bool {
        self.schema.columns().is_empty() && self.rows == [(TupleValue::empty(), Multiplicity::ONE)]
    }

    /// Fold equal rows into one entry and put the entries in a deterministic
    /// order, which is what makes the derived [`PartialEq`] bag equality.
    fn canonicalize(&mut self) -> Result<(), SyntaxError> {
        self.rows
            .sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
        let mut canonical: Vec<(TupleValue, Multiplicity)> = Vec::with_capacity(self.rows.len());
        for (row, copies) in std::mem::take(&mut self.rows) {
            match canonical.last_mut() {
                // Sorted, so equal rows are adjacent and the fold is local.
                Some((kept, kept_copies)) if *kept == row => {
                    *kept_copies = kept_copies.checked_add(copies).ok_or_else(|| {
                        SyntaxError::new(format!(
                            "the copies of row {} in a constant relation sum beyond u64",
                            row.data_to_string()
                        ))
                    })?;
                }
                _ => canonical.push((row, copies)),
            }
        }
        self.rows = canonical;
        Ok(())
    }

    /// Checks what the fields cannot enforce on their own.
    fn validate(&self) -> Result<(), SyntaxError> {
        self.validate_rows().and_then(|_| self.validate_keys())
    }

    /// Every row matches the declared columns, and every multiplicity is one a
    /// backend can carry.
    fn validate_rows(&self) -> Result<(), SyntaxError> {
        let columns = self.schema.columns();
        for (row, copies) in &self.rows {
            let arity = row.data.len();
            if arity != columns.len() {
                return Err(SyntaxError::new(format!(
                    "row {} of constant relation '{}' has {arity} value(s) \
                    but its schema declares {} column(s)",
                    row.data_to_string(),
                    self.schema.name(),
                    columns.len()
                )));
            }
            for (column, value) in columns.iter().zip(&row.data) {
                let actual = value.scalar_type();
                // A null stands in for a value of any declared type: nothing in
                // a `Column` claims otherwise, so rejecting it here would be
                // inventing a non-nullability the schema does not state.
                if actual != ScalarType::Null && actual != column.scalar_type() {
                    return Err(SyntaxError::new(format!(
                        "row {} of constant relation '{}' holds a {actual} in column \
                         '{}', which is declared as {}",
                        row.data_to_string(),
                        self.schema.name(),
                        column.name(),
                        column.scalar_type()
                    )));
                }
            }
            // Both backends carry a row's multiplicity as a *signed* 64 bit
            // weight, so a count beyond `i64::MAX` has nowhere to go. Catching
            // it here keeps the lowerings infallible on this point.
            if copies.get() > i64::MAX as u64 {
                return Err(SyntaxError::new(format!(
                    "row {} of constant relation '{}' is held {} times, beyond \
                     the signed 64 bit weight a backend carries it as",
                    row.data_to_string(),
                    self.schema.name(),
                    copies.get(),
                )));
            }
        }
        Ok(())
    }

    /// Every declared key must actually be one: no two distinct rows may share
    /// the same key.
    ///
    /// Only declared keys are claims. A schema declaring none lowers to the
    /// empty key, which indexes every row under the same (empty) key and asserts
    /// nothing; [`unit`](Self::unit) is that case.
    fn validate_keys(&self) -> Result<(), SyntaxError> {
        let key_of = |key: &[usize], row: &TupleValue| -> Vec<ScalarTypedValue> {
            key.iter().map(|index| row.data[*index].clone()).collect()
        };
        for key in self.schema.primary_key_indices() {
            // The rows are canonical, so equal rows have already been folded
            // into one entry: two entries agreeing on a key are two *distinct*
            // rows, which is the violation.
            let mut seen: HashMap<Vec<ScalarTypedValue>, &TupleValue> =
                HashMap::with_capacity(self.rows.len());
            for (row, _) in &self.rows {
                if let Some(previous) = seen.insert(key_of(key, row), row)
                    && previous != row
                {
                    let key_columns = key
                        .iter()
                        .map(|index| self.schema.columns()[*index].name())
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(SyntaxError::new(format!(
                        "rows {} and {} of constant relation '{}' agree on its declared \
                         key ({key_columns}), so that key does not determine a row",
                        previous.data_to_string(),
                        row.data_to_string(),
                        self.schema.name()
                    )));
                }
            }
        }
        Ok(())
    }
}

/// The shape and the rows, as `(a: uint) key(a) [[1]×2, [2]]`. Long constants
/// are elided after [`RENDERED_ROWS`] rows: this renders into plan dumps and
/// diagnostics, where the point is to recognise the relation, not to read it
/// out.
impl std::fmt::Display for ConstantExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} [", self.schema.shape())?;
        for (position, (row, copies)) in self.rows.iter().take(RENDERED_ROWS).enumerate() {
            if position > 0 {
                f.write_str(", ")?;
            }
            f.write_str(&row.data_to_string())?;
            // A `×1` on every row would drown out the ones that carry copies.
            if *copies != Multiplicity::ONE {
                write!(f, "{copies}")?;
            }
        }
        match self.rows.len().saturating_sub(RENDERED_ROWS) {
            0 => f.write_str("]"),
            elided => write!(f, ", … {elided} more]"),
        }
    }
}

/// Backend-neutral identity of a query output. Mirrors [`SourceId`] on the input
/// side: the plan only ever *names* a sink; the backend maps the name to a live
/// destination (a read handle, a CLI printer, …) at execution time.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SinkId(pub String);

impl SinkId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<T: Into<String>> From<T> for SinkId {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl std::fmt::Display for SinkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Where an [`OutputExpr`] sends the rows it taps. Pure data — the backend binds
/// each variant to a concrete destination; nothing runtime-stateful lives in the
/// plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum OutputKind {
    /// Print the (intermediate) rows to the CLI for debugging. The rows still
    /// flow downstream unchanged.
    Cli,
    /// Expose the rows as a named runtime output channel the driver reads via
    /// `Runtime::output`.
    Channel,
}

/// Taps a relation for output. This is a **pass-through** operator: it evaluates
/// to its input [`relation`](Self::relation) unchanged, so it can sit at the root
/// of a plan or splice into the middle of one (e.g. an [`OutputKind::Cli`] tap on
/// an intermediate result). The backend discovers every `OutputExpr` by walking
/// the plan and wires a destination for its [`SinkId`], exactly as it wires a
/// [`SourceExpr`] leaf on the input side.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputExpr {
    /// The relation to tap. Returned unchanged so downstream operators are
    /// unaffected by the tap.
    pub relation: Expr,
    /// The name this output is addressed by (`Runtime::output`, CLI label).
    pub id: SinkId,
    /// Where the tapped rows go.
    pub kind: OutputKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AliasExpr {
    pub relation: Expr,
    pub alias: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistinctExpr {
    pub relation: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnionExpr {
    /// All `Expr`s must evaluate to a relation and have a compatible schema,
    /// that is, the same order and arity of attributes with same types, respectively.
    pub relations: Vec<Expr>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DifferenceExpr {
    /// All `Expr`s must evaluate to a relation and have a compatible schema,
    /// that is, the same order and arity of attributes with same types, respectively.
    pub left: Expr,
    pub right: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionExpr {
    /// Must evaluate to a relation.
    pub relation: Expr,
    pub condition: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionExpr {
    /// Must evaluate to a relation.
    pub relation: Expr,
    /// The attributes to map over. The first element `String` is the name
    /// of the attribute. The second element `Expr` is the expression
    /// which produces the new value of the attribute.
    ///
    /// In case the `Expr` is just a `VarExpr` referencing a **tuple** variable,
    /// the interpreter is not run to evaluate the expression but instead only
    /// the schema is changed.
    pub attributes: Vec<(String, Expr)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CartesianProductExpr {
    /// We delegate to an [`EquiJoinExpr`] with an empty `on` clause.
    pub inner: EquiJoinExpr,
}

impl CartesianProductExpr {
    pub fn new(left: Expr, right: Expr, attributes: Option<Vec<(String, Expr)>>) -> Self {
        Self {
            inner: EquiJoinExpr {
                left,
                right,
                on: vec![],
                attributes,
            },
        }
    }
}

/// An equijoin is a join that exclusively uses equality of attribute(s).
/// [More information on join classifications](https://stackoverflow.com/a/7870216).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EquiJoinExpr {
    /// Must evaluate to a relation.
    pub left: Expr,
    /// Must evaluate to a relation.
    pub right: Expr,
    /// The attribute(s) to join on. The first element of any pair is evaluated
    /// in the context of the left relation, and the second element of any pair
    /// is evaluated in the context of the right relation.
    ///
    /// If `on` is empty, a [`CartesianProduct`](CartesianProductExpr) is computed.
    pub on: Vec<(Expr, Expr)>,
    /// An optional projection step. See documentation of [`ProjectionExpr`].
    pub attributes: Option<Vec<(String, Expr)>>,
}

/// The position of a relation within [`MultiWayEquiJoinExpr::relations`].
pub type RelationIdx = usize;

/// One equality class of a [`MultiWayEquiJoinExpr`]: every listed occurrence
/// must produce the same value for a tuple to enter the join's output.
///
/// A variable bound by only *one* relation is deliberately not representable
/// here: It constrains nothing, so it is not part of a join condition. Such a
/// variable still reaches the output, carried by its relation's schema like any
/// other non-join attribute. Keeping them out is what makes
/// [`MultiWayEquiJoinExpr::on`]`.is_empty()` an exact test for "nothing to join
/// on".
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JoinVariable {
    /// The name the joined attribute carries in the output schema.
    ///
    /// The lowering from coln's FLIR projects every atom onto the names of the
    /// variables it binds, so there the occurrences are plain column picks that
    /// already agree on this name, and the schema fold described on
    /// [`MultiWayEquiJoinExpr::on`] keeps exactly one active copy of it. When
    /// the occurrences do *not* agree on a name (`l.a = r.b`), producing this
    /// name is the job of whoever lowers the join.
    pub name: String,
    /// Which relations bind this variable, and how: the [`RelationIdx`] indexes
    /// into [`MultiWayEquiJoinExpr::relations`], and the [`Expr`] is evaluated
    /// in the context of that relation.
    ///
    /// Invariants, enforced by [`MultiWayEquiJoinExpr::new`]: at least two
    /// occurrences, every index in bounds, indices pairwise distinct, and
    /// ordered by index.
    pub occurrences: Vec<(RelationIdx, Expr)>,
}

/// An equijoin involving `N >= 2` relations. A better input than a folded
/// sequence of [binary `EquiJoin`s](EquiJoinExpr) for worst-case optimal join
/// algorithms (such as the leapfrog triejoin), which are variable-oriented:
/// they iterate a variable ordering, which is what [`on`](Self::on) spells out.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiWayEquiJoinExpr {
    /// The `N >= 2` relations which participate in the join. Each [`Expr`] must
    /// evaluate to a relation.
    pub relations: Vec<Expr>,
    /// The join condition, as one [`JoinVariable`] per equality class of
    /// attributes that have to agree.
    ///
    /// If `on` is empty, a [`CartesianProduct`](CartesianProductExpr) over
    /// [`relations`](Self::relations) is computed. Since a variable bound by a
    /// single relation cannot be a [`JoinVariable`], that test is exact rather
    /// than approximate.
    ///
    /// **Output schema.** Joining folds
    /// [`StreamSchema::join`](crate::relational::incremental::schema::StreamSchema::join)
    /// left to right, which deactivates an attribute of a later relation when
    /// an earlier one already contributes an active attribute of the same name.
    /// A join variable whose occurrences agree on their name therefore appears
    /// **once** in the output, carried by the first relation that binds it.
    /// No de-duplicating projection is required, and no join column is silently
    /// duplicated.
    pub on: Vec<JoinVariable>,
    /// An optional projection step. See documentation of [`ProjectionExpr`].
    pub attributes: Option<Vec<(String, Expr)>>,
}

impl MultiWayEquiJoinExpr {
    /// The only constructor that cannot produce a malformed join: it normalizes
    /// each [`JoinVariable`]'s occurrences into relation order and then applies
    /// [`validate`](Self::validate).
    pub fn new(
        relations: Vec<Expr>,
        on: Vec<JoinVariable>,
        attributes: Option<Vec<(String, Expr)>>,
    ) -> Result<Self, SyntaxError> {
        let mut joined = Self {
            relations,
            on,
            attributes,
        };
        for variable in &mut joined.on {
            variable.occurrences.sort_by_key(|(relation, _)| *relation);
        }
        joined.validate()?;
        Ok(joined)
    }

    /// Checks the invariants documented on [`Self::relations`] and
    /// [`JoinVariable::occurrences`]. [`Self::new`] applies this to everything
    /// it builds; the resolver re-applies it because the fields are public and
    /// a plan may also be assembled or rewritten by hand.
    pub fn validate(&self) -> Result<(), SyntaxError> {
        if self.relations.len() < 2 {
            return Err(SyntaxError::new(format!(
                "A multi way equi join requires at least two relations, got {}",
                self.relations.len()
            )));
        }
        let mut names = HashSet::with_capacity(self.on.len());
        for variable in &self.on {
            if !names.insert(&variable.name) {
                return Err(SyntaxError::new(format!(
                    "Join variable '{}' is declared twice",
                    variable.name
                )));
            }
            if variable.occurrences.len() < 2 {
                return Err(SyntaxError::new(format!(
                    "Join variable '{}' has {} occurrence(s): below two it constrains \
                     nothing, and a variable bound by a single relation reaches the \
                     output through that relation's schema instead",
                    variable.name,
                    variable.occurrences.len()
                )));
            }
            let mut relations = HashSet::with_capacity(variable.occurrences.len());
            for (relation, _) in &variable.occurrences {
                if *relation >= self.relations.len() {
                    return Err(SyntaxError::new(format!(
                        "Join variable '{}' refers to relation {relation} but the join \
                         has only {} relations",
                        variable.name,
                        self.relations.len()
                    )));
                }
                if !relations.insert(relation) {
                    return Err(SyntaxError::new(format!(
                        "Join variable '{}' occurs twice in relation {relation}: a \
                         variable repeated within one relation is a local equality \
                         condition on that relation, not a join condition",
                        variable.name
                    )));
                }
            }
        }
        Ok(())
    }

    /// Every [`Expr`] nested in the join condition, in [`on`](Self::on) order.
    /// Each one is evaluated in the context of *its own* relation, so a consumer
    /// that needs to know which relation must iterate [`on`](Self::on) directly.
    pub fn on_exprs(&self) -> impl Iterator<Item = &Expr> {
        self.on
            .iter()
            .flat_map(|variable| variable.occurrences.iter().map(|(_, expr)| expr))
    }

    /// The [`on_exprs`](Self::on_exprs) counterpart for rewriting passes.
    /// Handing out `&mut Expr` cannot break any invariant, as those constrain
    /// the arity and the relation indices rather than the expressions.
    pub fn on_exprs_mut(&mut self) -> impl Iterator<Item = &mut Expr> {
        self.on
            .iter_mut()
            .flat_map(|variable| variable.occurrences.iter_mut().map(|(_, expr)| expr))
    }
}

/// This is not a commutative operation, that is, swapping the `left` and `right`
/// relations may alter the result. This computes `left` setminus `right` while
/// only considering the columns specified in `on`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AntiJoinExpr {
    /// Must evaluate to a relation.
    pub left: Expr,
    /// Must evaluate to a relation.
    pub right: Expr,
    /// The attributes the two relations are compared on: a `left` row is
    /// suppressed exactly when some `right` row agrees with it on all of them.
    /// The first element of any pair is evaluated in the context of the left
    /// relation, the second in the context of the right one, and each pair
    /// should produce the same type.
    ///
    /// Note that this is the key to match *on*, in the same sense as
    /// [`EquiJoinExpr::on`] — the columns that survive into the output are not
    /// expressed here at all, since the output carries the left relation's
    /// schema unchanged.
    pub on: Vec<(Expr, Expr)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThetaJoinExpr {
    // Can be subsumed by the EquiJoin/MultiWayJoin, but better
    /// Must evaluate to a relation.
    pub left: Expr,
    /// Must evaluate to a relation.
    pub right: Expr,
    /// An arbitrary join condition that is evaluated for each pair of tuples
    /// (cartesian product of both relations) in the context containing
    /// the attributes of both relations.
    /// If the condition evaluates to false, the output does not contain that
    /// pair. If the condition evaluates to true, the output contains the pair.
    pub on: Expr,
    /// An optional projection step. See documentation of [`ProjectionExpr`].
    pub attributes: Option<Vec<(String, Expr)>>,
}

/// Evaluates to a relation/stream again.
///
/// The step statements may freely reference relations defined in the enclosing
/// scope as well as [`SourceExpr`] leaves; the backend is responsible for
/// bringing those into the iteration (in the DBSP backend, via `delta0`). This
/// keeps the node declarative: it describes *what* is iterated, not *how* the
/// backend must wire outer relations in.
#[derive(Clone, Debug)]
pub struct FixedPointIterExpr {
    /// The accumulator is available as a variable named according to the first
    /// tuple element in the context of the child circuit, that is,
    /// within the the context of the [`step`](FixedPointIterExpr.step) statements.
    /// The second tuple element must evaluate to a relation.
    /// The accumulator also defines the schema of the fixed point computation.
    pub accumulator: (String, Expr),
    /// What to do in each iteration. Runs in the context of the child circuit.
    /// The value the last statement evaluates to becomes the accumulator of
    /// the next iteration.
    pub step: BlockStmt,
}

impl Eq for FixedPointIterExpr {}

impl PartialEq for FixedPointIterExpr {
    fn eq(&self, other: &Self) -> bool {
        self.accumulator == other.accumulator && self.step == other.step
    }
}

/// Pure dispatch contract for the relational plan layer: a provided `visit_rel`
/// router plus one required method per operator. No semantics are baked in; each
/// backend implements the per-node methods with its own return type `T`.
pub trait RelExprVisitor<T, C> {
    fn visit_rel(&mut self, expr: &RelExpr, ctx: C) -> T {
        match expr {
            RelExpr::Source(expr) => self.visit_source_expr(expr, ctx),
            RelExpr::Constant(expr) => self.visit_constant_expr(expr, ctx),
            RelExpr::Output(expr) => self.visit_output_expr(expr, ctx),
            RelExpr::Alias(expr) => self.visit_alias_expr(expr, ctx),
            RelExpr::Distinct(expr) => self.visit_distinct_expr(expr, ctx),
            RelExpr::Union(expr) => self.visit_union_expr(expr, ctx),
            RelExpr::Difference(expr) => self.visit_difference_expr(expr, ctx),
            RelExpr::Selection(expr) => self.visit_selection_expr(expr, ctx),
            RelExpr::Projection(expr) => self.visit_projection_expr(expr, ctx),
            RelExpr::CartesianProduct(expr) => self.visit_cartesian_product_expr(expr, ctx),
            RelExpr::EquiJoin(expr) => self.visit_equi_join_expr(expr, ctx),
            RelExpr::MultiWayEquiJoin(expr) => self.visit_multi_way_equi_join_expr(expr, ctx),
            RelExpr::AntiJoin(expr) => self.visit_anti_join_expr(expr, ctx),
            RelExpr::FixedPointIter(expr) => self.visit_fixed_point_iter_expr(expr, ctx),
        }
    }
    fn visit_source_expr(&mut self, expr: &SourceExpr, ctx: C) -> T;
    fn visit_constant_expr(&mut self, expr: &ConstantExpr, ctx: C) -> T;
    fn visit_output_expr(&mut self, expr: &OutputExpr, ctx: C) -> T;
    fn visit_alias_expr(&mut self, expr: &AliasExpr, ctx: C) -> T;
    fn visit_distinct_expr(&mut self, expr: &DistinctExpr, ctx: C) -> T;
    fn visit_union_expr(&mut self, expr: &UnionExpr, ctx: C) -> T;
    fn visit_difference_expr(&mut self, expr: &DifferenceExpr, ctx: C) -> T;
    fn visit_selection_expr(&mut self, expr: &SelectionExpr, ctx: C) -> T;
    fn visit_projection_expr(&mut self, expr: &ProjectionExpr, ctx: C) -> T;
    fn visit_cartesian_product_expr(&mut self, expr: &CartesianProductExpr, ctx: C) -> T;
    fn visit_equi_join_expr(&mut self, expr: &EquiJoinExpr, ctx: C) -> T;
    fn visit_multi_way_equi_join_expr(&mut self, expr: &MultiWayEquiJoinExpr, ctx: C) -> T;
    fn visit_anti_join_expr(&mut self, expr: &AntiJoinExpr, ctx: C) -> T;
    fn visit_fixed_point_iter_expr(&mut self, expr: &FixedPointIterExpr, ctx: C) -> T;
}

/// Annotating visitor. See [`RelExprVisitorOwn`].
pub trait RelExprVisitorMut<T, C> {
    fn visit_rel(&mut self, expr: &mut RelExpr, ctx: C) -> T {
        match expr {
            RelExpr::Source(expr) => self.visit_source_expr(expr, ctx),
            RelExpr::Constant(expr) => self.visit_constant_expr(expr, ctx),
            RelExpr::Output(expr) => self.visit_output_expr(expr, ctx),
            RelExpr::Alias(expr) => self.visit_alias_expr(expr, ctx),
            RelExpr::Distinct(expr) => self.visit_distinct_expr(expr, ctx),
            RelExpr::Union(expr) => self.visit_union_expr(expr, ctx),
            RelExpr::Difference(expr) => self.visit_difference_expr(expr, ctx),
            RelExpr::Selection(expr) => self.visit_selection_expr(expr, ctx),
            RelExpr::Projection(expr) => self.visit_projection_expr(expr, ctx),
            RelExpr::CartesianProduct(expr) => self.visit_cartesian_product_expr(expr, ctx),
            RelExpr::EquiJoin(expr) => self.visit_equi_join_expr(expr, ctx),
            RelExpr::MultiWayEquiJoin(expr) => self.visit_multi_way_equi_join_expr(expr, ctx),
            RelExpr::AntiJoin(expr) => self.visit_anti_join_expr(expr, ctx),
            RelExpr::FixedPointIter(expr) => self.visit_fixed_point_iter_expr(expr, ctx),
        }
    }
    fn visit_source_expr(&mut self, expr: &mut SourceExpr, ctx: C) -> T;
    fn visit_constant_expr(&mut self, expr: &mut ConstantExpr, ctx: C) -> T;
    fn visit_output_expr(&mut self, expr: &mut OutputExpr, ctx: C) -> T;
    fn visit_alias_expr(&mut self, expr: &mut AliasExpr, ctx: C) -> T;
    fn visit_distinct_expr(&mut self, expr: &mut DistinctExpr, ctx: C) -> T;
    fn visit_union_expr(&mut self, expr: &mut UnionExpr, ctx: C) -> T;
    fn visit_difference_expr(&mut self, expr: &mut DifferenceExpr, ctx: C) -> T;
    fn visit_selection_expr(&mut self, expr: &mut SelectionExpr, ctx: C) -> T;
    fn visit_projection_expr(&mut self, expr: &mut ProjectionExpr, ctx: C) -> T;
    fn visit_cartesian_product_expr(&mut self, expr: &mut CartesianProductExpr, ctx: C) -> T;
    fn visit_equi_join_expr(&mut self, expr: &mut EquiJoinExpr, ctx: C) -> T;
    fn visit_multi_way_equi_join_expr(&mut self, expr: &mut MultiWayEquiJoinExpr, ctx: C) -> T;
    fn visit_anti_join_expr(&mut self, expr: &mut AntiJoinExpr, ctx: C) -> T;
    fn visit_fixed_point_iter_expr(&mut self, expr: &mut FixedPointIterExpr, ctx: C) -> T;
}

/// Restructuring visitor for the relational layer, and the family a
/// backend-specific *lowering* pass lives in — see
/// [`ExprVisitorOwn`](crate::host::expr::ExprVisitorOwn) for the rule that
/// decides between the three families, and for why the payloads arrive boxed.
pub trait RelExprVisitorOwn<T, C> {
    fn visit_rel(&mut self, expr: RelExpr, ctx: C) -> T {
        match expr {
            RelExpr::Source(expr) => self.visit_source_expr(expr, ctx),
            RelExpr::Constant(expr) => self.visit_constant_expr(expr, ctx),
            RelExpr::Output(expr) => self.visit_output_expr(expr, ctx),
            RelExpr::Alias(expr) => self.visit_alias_expr(expr, ctx),
            RelExpr::Distinct(expr) => self.visit_distinct_expr(expr, ctx),
            RelExpr::Union(expr) => self.visit_union_expr(expr, ctx),
            RelExpr::Difference(expr) => self.visit_difference_expr(expr, ctx),
            RelExpr::Selection(expr) => self.visit_selection_expr(expr, ctx),
            RelExpr::Projection(expr) => self.visit_projection_expr(expr, ctx),
            RelExpr::CartesianProduct(expr) => self.visit_cartesian_product_expr(expr, ctx),
            RelExpr::EquiJoin(expr) => self.visit_equi_join_expr(expr, ctx),
            RelExpr::MultiWayEquiJoin(expr) => self.visit_multi_way_equi_join_expr(expr, ctx),
            RelExpr::AntiJoin(expr) => self.visit_anti_join_expr(expr, ctx),
            RelExpr::FixedPointIter(expr) => self.visit_fixed_point_iter_expr(expr, ctx),
        }
    }
    fn visit_source_expr(&mut self, expr: Box<SourceExpr>, ctx: C) -> T;
    fn visit_constant_expr(&mut self, expr: Box<ConstantExpr>, ctx: C) -> T;
    fn visit_output_expr(&mut self, expr: Box<OutputExpr>, ctx: C) -> T;
    fn visit_alias_expr(&mut self, expr: Box<AliasExpr>, ctx: C) -> T;
    fn visit_distinct_expr(&mut self, expr: Box<DistinctExpr>, ctx: C) -> T;
    fn visit_union_expr(&mut self, expr: Box<UnionExpr>, ctx: C) -> T;
    fn visit_difference_expr(&mut self, expr: Box<DifferenceExpr>, ctx: C) -> T;
    fn visit_selection_expr(&mut self, expr: Box<SelectionExpr>, ctx: C) -> T;
    fn visit_projection_expr(&mut self, expr: Box<ProjectionExpr>, ctx: C) -> T;
    fn visit_cartesian_product_expr(&mut self, expr: Box<CartesianProductExpr>, ctx: C) -> T;
    fn visit_equi_join_expr(&mut self, expr: Box<EquiJoinExpr>, ctx: C) -> T;
    fn visit_multi_way_equi_join_expr(&mut self, expr: Box<MultiWayEquiJoinExpr>, ctx: C) -> T;
    fn visit_anti_join_expr(&mut self, expr: Box<AntiJoinExpr>, ctx: C) -> T;
    fn visit_fixed_point_iter_expr(&mut self, expr: Box<FixedPointIterExpr>, ctx: C) -> T;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::expr::VarExpr;
    use crate::test_utils::table_schema;
    use crate::tuple;

    /// A stand-in relation operand. [`MultiWayEquiJoinExpr::validate`] only ever
    /// counts these, so their content is irrelevant.
    fn relations(count: usize) -> Vec<Expr> {
        (0..count)
            .map(|idx| Expr::from(VarExpr::new(format!("r{idx}"))))
            .collect()
    }

    fn join_variable(name: &str, occurrences: &[RelationIdx]) -> JoinVariable {
        JoinVariable {
            name: name.to_string(),
            occurrences: occurrences
                .iter()
                .map(|relation| (*relation, Expr::from(VarExpr::new(name))))
                .collect(),
        }
    }

    /// A two-column schema, keyed on `key`.
    fn keyed_schema(key: &[&str]) -> TableSchema {
        table_schema(
            "pairs",
            [("a", ScalarType::Uint), ("b", ScalarType::Bool)],
            key.iter().copied(),
        )
    }

    fn copies(count: u64) -> Multiplicity {
        Multiplicity::new(count).expect("a positive number of copies")
    }

    #[test]
    fn folds_repeated_rows_into_one_entry_with_their_multiplicity() {
        // The bag semantics of the node: listing a row twice is not an error and
        // not a duplicate entry, it is two copies of one row. That is what makes
        // a `Multiplicity` the only place a count lives.
        let constant = ConstantExpr::set(
            keyed_schema(&[]),
            [tuple!(1u64, true), tuple!(2u64, false), tuple!(1u64, true)],
        )
        .expect("repeated rows are a bag, not a violation");
        assert_eq!(
            constant.rows(),
            [
                (tuple!(1u64, true), copies(2)),
                (tuple!(2u64, false), Multiplicity::ONE),
            ]
        );
    }

    #[test]
    fn equality_is_bag_equality_rather_than_listing_order() {
        // What the canonical form buys: a rewrite rule recognising two
        // occurrences of one constant, and a plan assertion that does not depend
        // on the order a producer happened to emit the rows in.
        let schema = keyed_schema(&[]);
        let one = ConstantExpr::set(schema.clone(), [tuple!(1u64, true), tuple!(2u64, false)])
            .expect("valid rows");
        let other = ConstantExpr::set(schema, [tuple!(2u64, false), tuple!(1u64, true)])
            .expect("valid rows");
        assert_eq!(one, other);
    }

    #[test]
    fn rejects_a_row_whose_arity_disagrees_with_the_schema() {
        let error = ConstantExpr::set(keyed_schema(&[]), [tuple!(1u64)])
            .expect_err("a one-value row does not fit a two-column schema");
        assert!(error.to_string().contains("column"));
    }

    #[test]
    fn rejects_a_cell_whose_type_disagrees_with_its_column() {
        let error = ConstantExpr::set(keyed_schema(&[]), [tuple!(1u64, 2u64)])
            .expect_err("a uint does not fit a bool column");
        assert!(error.to_string().contains("bool"));
    }

    #[test]
    fn accepts_a_null_in_a_column_of_any_type() {
        // Nothing in a `Column` states non-nullability, so rejecting this would
        // be inventing a constraint the schema does not carry.
        ConstantExpr::set(
            keyed_schema(&["a"]),
            [TupleValue::new(vec![
                ScalarTypedValue::Uint(1),
                ScalarTypedValue::Null(()),
            ])],
        )
        .expect("a null stands in for a value of the declared type");
    }

    #[test]
    fn rejects_two_distinct_rows_agreeing_on_a_declared_key() {
        // The invariant no backend can enforce: an `OrdIndexedZSet` is an index,
        // so a key holding several distinct values is legal there. A constant's
        // rows are in hand, so the claim is decidable here.
        let error = ConstantExpr::set(
            keyed_schema(&["a"]),
            [tuple!(1u64, true), tuple!(1u64, false)],
        )
        .expect_err("a declared key must determine the row");
        assert!(error.to_string().contains("key"));
    }

    #[test]
    fn copies_of_one_row_do_not_violate_a_declared_key() {
        // Two *entries* under one key are a violation; two *copies* of one row
        // are the multiplicity of that row, which the fold has already
        // collapsed into a single entry by the time the key is checked.
        let constant = ConstantExpr::set(
            keyed_schema(&["a"]),
            [tuple!(1u64, true), tuple!(1u64, true)],
        )
        .expect("copies of one row agree on the key trivially");
        assert_eq!(constant.rows(), [(tuple!(1u64, true), copies(2))]);
    }

    #[test]
    fn an_undeclared_key_claims_nothing() {
        // Without a declared key the relation lowers to the empty key, which
        // indexes every row under the same key and asserts nothing — so rows
        // that would violate a key on `a` are fine here.
        ConstantExpr::set(keyed_schema(&[]), [tuple!(1u64, true), tuple!(1u64, false)])
            .expect("no declared key, no claim to violate");
    }

    #[test]
    fn the_unit_relation_holds_exactly_one_copy_of_the_unit_tuple() {
        let unit = ConstantExpr::unit();
        assert!(unit.schema().columns().is_empty());
        assert_eq!(unit.rows(), [(TupleValue::empty(), Multiplicity::ONE)]);
        assert!(unit.is_unit());
    }

    #[test]
    fn the_unit_tuple_held_twice_is_not_the_joins_neutral_element() {
        // A join against it would scale its other operand by two, so a
        // join-elimination rule must not fire on it. Hence `is_unit` tests the
        // multiplicity, not just the shape.
        let twice = ConstantExpr::new(
            table_schema("unit", [], []),
            [(TupleValue::empty(), copies(2))],
        )
        .expect("a bag may hold the unit tuple twice");
        assert!(!twice.is_unit());
    }

    #[test]
    fn an_empty_constant_keeps_the_arity_its_schema_declares() {
        // Why the schema is declared rather than inferred: there is no row here
        // to read an arity off.
        let empty = ConstantExpr::empty(keyed_schema(&["a"]));
        assert!(empty.is_empty());
        assert_eq!(empty.schema().columns().len(), 2);
    }

    #[test]
    fn accepts_a_join_variable_shared_by_two_relations() {
        let joined =
            MultiWayEquiJoinExpr::new(relations(2), vec![join_variable("x", &[0, 1])], None)
                .expect("A variable bound by two relations is a join variable");
        assert_eq!(joined.on.len(), 1);
        assert_eq!(joined.on_exprs().count(), 2);
    }

    #[test]
    fn accepts_an_empty_join_condition_as_a_cartesian_product() {
        let joined = MultiWayEquiJoinExpr::new(relations(3), vec![], None)
            .expect("An empty join condition is a cartesian product, not an error");
        assert!(joined.on.is_empty());
    }

    #[test]
    fn rejects_fewer_than_two_relations() {
        for count in 0..2 {
            assert!(
                MultiWayEquiJoinExpr::new(relations(count), vec![], None).is_err(),
                "A join over {count} relation(s) should be rejected"
            );
        }
    }

    #[test]
    fn rejects_a_single_occurrence_because_it_constrains_nothing() {
        // The whole point of the `on` representation: a variable bound by only
        // one relation is not an equality class. It reaches the output through
        // that relation's schema instead, which is why rejecting it here is safe
        // and keeps `on.is_empty()` an exact cartesian-product test.
        let error = MultiWayEquiJoinExpr::new(relations(2), vec![join_variable("x", &[0])], None)
            .expect_err("A single occurrence must not be representable");
        assert!(error.to_string().contains("occurrence"));
    }

    #[test]
    fn rejects_an_out_of_bounds_relation_index() {
        assert!(
            MultiWayEquiJoinExpr::new(relations(2), vec![join_variable("x", &[0, 2])], None)
                .is_err()
        );
    }

    #[test]
    fn rejects_a_variable_occurring_twice_in_one_relation() {
        // Such a repetition is a local equality condition on that one relation,
        // so it belongs in a `SelectionExpr` beneath the join.
        assert!(
            MultiWayEquiJoinExpr::new(relations(2), vec![join_variable("x", &[0, 0])], None)
                .is_err()
        );
    }

    #[test]
    fn rejects_two_join_variables_claiming_the_same_output_name() {
        assert!(
            MultiWayEquiJoinExpr::new(
                relations(3),
                vec![join_variable("x", &[0, 1]), join_variable("x", &[1, 2])],
                None
            )
            .is_err()
        );
    }

    #[test]
    fn normalizes_occurrences_into_relation_order() {
        // Plans have to be reproducible: the occurrence order must not depend on
        // the order the producer happened to discover the occurrences in.
        let joined =
            MultiWayEquiJoinExpr::new(relations(3), vec![join_variable("x", &[2, 0, 1])], None)
                .expect("Out-of-order occurrences are normalized, not rejected");
        let order: Vec<RelationIdx> = joined.on[0]
            .occurrences
            .iter()
            .map(|(relation, _)| *relation)
            .collect();
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn validate_agrees_with_new_on_hand_assembled_joins() {
        // The fields are public, so a hand-built or rewritten plan can violate
        // the invariants; the resolver relies on `validate` catching that.
        let malformed = MultiWayEquiJoinExpr {
            relations: relations(2),
            on: vec![join_variable("x", &[0])],
            attributes: None,
        };
        assert!(malformed.validate().is_err());
    }
}
