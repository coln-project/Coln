// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! What a plan's source leaves *mean*.
//!
//! A [`SourceExpr`](crate::relational::expr::SourceExpr) leaf only names an
//! extensional relation. Everything else about that relation — today its
//! [`TableSchema`], tomorrow perhaps cardinality estimates a cost-based
//! optimizer would want — is answered by the [`Catalog`] the plan is compiled
//! against, so that a relation referenced `N` times is still described once.
//!
//! What a catalog answers with is the *neutral* schema: columns, types, and the
//! table's key(s). Turning that into the keyed, positional layout a particular
//! runtime needs is the backend's job (see
//! [`StreamSchema`](crate::relational::incremental::StreamSchema) for the
//! DBSP one).

use crate::{
    error::BuildError,
    host::{
        stmt::Stmt,
        walk::{Node, pre_order},
    },
    relational::{expr::SourceId, schema::TableSchema},
};
use std::borrow::Cow;
use std::collections::HashMap;

/// The static description of a plan's extensional inputs, keyed by the
/// [`SourceId`] its leaves carry.
///
/// Read-only and longer-lived than the plan itself: the optimizer, the lowering
/// and the resolver all rewrite the code, while the catalog they consult stays
/// as it is — which is why this is a trait of its own rather than a method on
/// [`QueryProgram`](crate::program::QueryProgram), whose code gets moved out
/// from under it. A pass is handed the `&dyn Catalog` half and so cannot touch
/// the code half.
pub trait Catalog {
    /// The schema of the relation `id` names, or [`None`] if this catalog does
    /// not describe it.
    ///
    /// Returns a [`Cow`] so that an implementation is free to keep a *richer*
    /// per-relation description of its own, e.g., coln's FLIR frontend stores a
    /// `BaseTableSchema`, with one column view per engine and the index
    /// translations between them, and project it down on demand
    /// ([`Cow::Owned`]), instead of being forced to store a parallel
    /// [`TableSchema`] just to have one to lend out. An implementation that
    /// does hold one lends it ([`Cow::Borrowed`]) and allocates nothing.
    fn source_schema(&self, id: &SourceId) -> Option<Cow<'_, TableSchema>>;
}

/// Every source *one plan* names, with the schema its catalog describes it by:
/// the resolved projection of a [`Catalog`] onto that plan. One entry per
/// *distinct* [`SourceId`], however many leaves reference it.
///
/// This is what a [`Backend`](crate::relational::Backend) is handed, rather than
/// the catalog itself — see [`resolve_sources`] for why resolving up front is
/// what makes an incremental circuit buildable at all.
pub type SourceSchemas = HashMap<SourceId, TableSchema>;

/// A resolved projection answers the same questions the [`Catalog`] it came from
/// does, so a consumer that only wants to look a source up — the type resolver,
/// the tree printer — takes a `&dyn Catalog` and can be handed either. One
/// vocabulary, whether the schemas are still to be computed or already resolved.
impl Catalog for SourceSchemas {
    fn source_schema(&self, id: &SourceId) -> Option<Cow<'_, TableSchema>> {
        self.get(id).map(Cow::Borrowed)
    }
}

/// Resolve every [`SourceExpr`](crate::relational::expr::SourceExpr) leaf in
/// `code` against `catalog`, up front. The single point at which a [`Catalog`]
/// is consulted: [`Pipeline::runtime`](crate::pipeline::Pipeline::runtime) calls
/// this once, and every stage downstream works from the [`SourceSchemas`] it
/// returns.
///
/// This is where a plan naming a relation the catalog knows nothing about is
/// caught and covers the one failure mode that name-only leaves introduce.
/// Hence, it fails before a backend has built anything, naming the offending
/// source rather than lazily discovering as a missing input later on.
///
/// Resolving eagerly is also what lets an incremental backend exist: DBSP's
/// `init_circuit` constructor must be `Send + 'static` (it runs once per worker
/// thread), so a borrowed catalog cannot cross into it, while owned schemas can.
pub fn resolve_sources(code: &[Stmt], catalog: &dyn Catalog) -> Result<SourceSchemas, BuildError> {
    pre_order(code)
        .filter_map(Node::as_source)
        .map(|source| {
            let schema = catalog.source_schema(source.as_id()).ok_or_else(|| {
                BuildError::new(format!(
                    "Source '{}' is not described by the catalog this plan is compiled \
                     against, so there is nothing to bind it to",
                    source.as_id()
                ))
            })?;
            Ok((source.as_id().clone(), schema.into_owned()))
        })
        .collect()
}
