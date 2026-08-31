// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! What a table *is*, said in a way no backend and no frontend owns.
//!
//! This is the middle of three schema vocabularies coln-query speaks:
//!
//! 1. A **frontend** schema knows how the table got here. coln's FLIR frontend
//!    has `BaseTableSchema`, which carries one column view per engine (compiler,
//!    store, query) and the index translations between them. It converts *into*
//!    the schema in this module.
//! 2. The **neutral** [`TableSchema`] here: named, typed columns in one order,
//!    plus the (compound) primary key(s) over them. This is what a
//!    [`Catalog`](super::catalog::Catalog) answers with, what the type resolver
//!    reads, and what a [`Backend`](super::Backend) is handed.
//! 3. A **backend** schema knows how the table is physically laid out for
//!    execution. The DBSP backend has
//!    [`StreamSchema`](super::incremental::StreamSchema), whose keyed
//!    `(TupleKey, TupleValue)` shape follows from `OrdIndexedZSet` rather than
//!    from anything about the table itself. Each backend converts the neutral
//!    schema into its own and keeps it inside its own relation representation.
//!
//! Layer 2 is deliberately the poorest of the three: a table has columns,
//! types, and key(s), and a relation may have several candidate keys or none.
//! Which one — if any — becomes a physical index is layer 3's decision.

use crate::{
    relational::expr::{SinkId, SourceId},
    scalarial::ScalarType,
};
use std::fmt::{self, Display};

/// An identifier that uniquely identifies a table (globally across the store).
#[derive(Eq, PartialEq, Hash, Debug, Clone)]
pub struct EntityRef {
    inner: String,
}

impl EntityRef {
    pub fn id(&self) -> &str {
        &self.inner
    }
}

impl Display for EntityRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

/// The other direction of the same identity: a plan's
/// [`OutputExpr`](crate::relational::expr::OutputExpr) leaf names its derived
/// view by a [`SinkId`] built from that table's name, so a `SinkId`
/// and the `TableRef` of the table it names hold the same string.
/// This is what lets a [`Catalog`](super::catalog::Catalog) lookup reach a
///  `TableRef`-keyed map.
impl From<&SinkId> for EntityRef {
    fn from(value: &SinkId) -> Self {
        EntityRef {
            inner: value.0.clone(),
        }
    }
}

/// The other direction of the same identity: a plan's
/// [`SourceExpr`](crate::relational::expr::SourceExpr) leaf names its base
/// table by a [`SourceId`] built from that table's name, so a `SourceId`
/// and the `TableRef` of the table it names hold the same string.
/// This is what lets a [`Catalog`](super::catalog::Catalog) lookup reach a
///  `TableRef`-keyed map.
impl From<&SourceId> for EntityRef {
    fn from(value: &SourceId) -> Self {
        EntityRef {
            inner: value.0.clone(),
        }
    }
}

impl From<&str> for EntityRef {
    fn from(value: &str) -> Self {
        EntityRef {
            inner: value.to_string(),
        }
    }
}

impl From<String> for EntityRef {
    fn from(value: String) -> Self {
        EntityRef { inner: value }
    }
}

impl From<&EntityRef> for SourceId {
    fn from(value: &EntityRef) -> Self {
        SourceId(value.inner.clone())
    }
}

/// The backend-neutral and frontend-neutral description of one table:
/// Its columns and the key(s) over them. See the [module docs](self) for the
/// layer this sits in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSchema {
    /// The table's unique identifier/name.
    name: EntityRef,
    /// All fields of the table in their physical order.
    columns: Vec<Column>,
    /// The list of (possibly compound) primary keys into the table, specified
    /// as indexes into the schema's [`columns`](Self::columns).
    ///
    /// Plural and possibly empty, unlike the single key a keyed backend needs:
    /// picking one of these (or synthesizing one) is that backend's business.
    primary_keys: Vec<Vec<usize>>,
}

impl TableSchema {
    pub fn new(name: EntityRef, columns: Vec<Column>, primary_keys: Vec<Vec<usize>>) -> Self {
        debug_assert!(
            primary_keys
                .iter()
                .flatten()
                .all(|idx| *idx < columns.len()),
            "primary key of table {name} indexes a column it does not have"
        );
        Self {
            name,
            columns,
            primary_keys,
        }
    }
    pub fn name(&self) -> &EntityRef {
        &self.name
    }
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }
    /// The (compound) primary key(s) expressed in [`columns`](Self::columns).
    pub fn primary_keys(&self) -> impl Iterator<Item = impl Iterator<Item = &Column>> {
        self.primary_keys.iter().map(|primary_key| {
            primary_key.iter().map(|idx| {
                self.columns
                    .get(*idx)
                    .expect("primary key indexes a column of its own schema")
            })
        })
    }
    /// Everything but the name: the typed columns and the key(s) over them, as
    /// `(a: uint, b: iint) key(a)`. What a plan printer wants, since a source
    /// leaf has already named the relation by the time its schema is rendered.
    pub fn shape(&self) -> impl Display {
        Shape(self)
    }
}

impl Display for TableSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.name, self.shape())
    }
}

struct Shape<'a>(&'a TableSchema);

impl Display for Shape<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let joined = |names: Vec<String>| names.join(", ");
        let columns = joined(self.0.columns.iter().map(Column::to_string).collect());
        write!(f, "({columns})")?;
        self.0.primary_keys().try_for_each(|primary_key| {
            let stringified = joined(
                primary_key
                    .map(|column| column.name().to_string())
                    .collect(),
            );
            write!(f, " key({stringified})")
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// The column's name.
    name: String,
    /// Ihe column's (scalar) type.
    scalar_type: ScalarType,
}

impl Column {
    pub fn new<N: Into<String>, T: Into<ScalarType>>(name: N, scalar_type: T) -> Self {
        Self {
            name: name.into(),
            scalar_type: scalar_type.into(),
        }
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn scalar_type(&self) -> ScalarType {
        self.scalar_type
    }
}

impl Display for Column {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.scalar_type)
    }
}
