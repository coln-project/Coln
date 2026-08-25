// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::relational::schema::TableSchema;
use crate::scalarial::{ScalarType, ScalarTypedValue};
use dbsp::{never_none, never_roaring_filter};
use std::{
    any::Any,
    cell::RefCell,
    collections::HashMap,
    fmt::{self, Debug, Display},
    rc::Rc,
};

pub trait Tuple: FromIterator<ScalarTypedValue> {
    fn empty() -> Self {
        Self::from_iter(vec![])
    }
    fn data_at(&self, index: usize) -> &ScalarTypedValue;
    /// Iterates over _all_ stored fields of the tuple,
    /// regardless if they are part of the current schema.
    fn data(&self) -> impl Iterator<Item = &ScalarTypedValue>;
    /// Assumes that the passed indexes are valid for the tuple.
    fn data_to_string(&self) -> String {
        let fields = self
            .data()
            .map(|field| field.to_string())
            .collect::<Vec<_>>()
            .join(" | ");
        format!("| {fields} |")
    }
}

#[derive(
    Clone,
    Default,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    size_of::SizeOf,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[archive_attr(derive(Ord, Eq, PartialEq, PartialOrd))]
pub struct TupleValue {
    /// The data of the tuple which can be accessed by the index.
    /// Currently, the fields store their types alongside the data. However,
    /// this is redundant and could be removed to save space. Interestingly,
    /// `ScalarValue`, which is a union without a type tag (contrast it
    /// with `ScalarTypedValue`), has the same size, hence, the extra type tag
    /// does not increase the size currently.
    pub data: Vec<ScalarTypedValue>,
}

never_none!(TupleValue);
never_roaring_filter!(TupleValue);

#[macro_export]
macro_rules! tuple {
    ( $( $key:expr ),* $(,)?) => {{
        let tuple = [$( ScalarTypedValue::from($key) ),*];
        TupleValue {
            data: tuple.to_vec(),
        }
    }};
}

impl<T: Into<ScalarTypedValue>> FromIterator<T> for TupleValue {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            data: iter.into_iter().map(|v| v.into()).collect(),
        }
    }
}

impl Tuple for TupleValue {
    fn data_at(&self, index: usize) -> &ScalarTypedValue {
        &self.data[index]
    }
    fn data(&self) -> impl Iterator<Item = &ScalarTypedValue> {
        self.data.iter()
    }
}

impl Display for TupleValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.data_to_string())
    }
}

/// Currently unused.
#[derive(Debug, Hash, Eq, PartialEq, Clone, PartialOrd, Ord)]
struct Identifier {
    name: String,
}

impl Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

/// Convenience type alias for a reference to a [`Relation`].
pub type RelationRef = Rc<RefCell<Relation>>;

pub fn new_relation<R: RelationData>(inner: R) -> RelationRef {
    Rc::new(RefCell::new(Relation::new(inner)))
}

/// The backend-neutral payload of a [`Relation`] — a type-erased envelope over a
/// backend's concrete relation representation (a DBSP `StreamWrapper`, a batch
/// Z-set, …).
///
/// This is layer 3 of the multi-backend split ("the only place `StreamWrapper`
/// vs a batch Z-set actually differs"). It carries **no algebra**: relational
/// operations live in each backend's
/// [`RelExprVisitor`](crate::relational::expr::RelExprVisitor), which recovers
/// its own concrete type via [`Relation::downcast_ref`]. Keeping this trait
/// algebra-free is what stops any backend's operator vocabulary from leaking
/// into the host `Value` type.
///
/// [`Display`] and [`Debug`] are required because the host layer prints
/// relations ([`Value::Relation`](crate::host::variable::Value)) without knowing
/// what one is: how a relation describes itself is the one thing a backend has
/// to tell the layer above, and its schema is what it says.
pub trait RelationData: Any + Display + Debug {
    fn as_any(&self) -> &dyn Any;
    fn clone_box(&self) -> Box<dyn RelationData>;
}

impl Clone for Box<dyn RelationData> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// A relation flowing through a plan, as the host layer sees it: an opaque
/// handle. Everything about *how* the relation is represented, including the
/// physical schema its rows are laid out by, belongs to the backend and lives
/// inside [`RelationData`], because a schema that says "key columns" already
/// says which backend is running.
#[derive(Clone)]
pub struct Relation {
    /// The backend's concrete relation, type-erased. Access it from within a
    /// backend via [`Self::downcast_ref`].
    inner: Box<dyn RelationData>,
}

impl Relation {
    pub fn new<R: RelationData>(inner: R) -> Self {
        Self {
            inner: Box::new(inner),
        }
    }
    /// Recover the backend's concrete relation type. Called only from within a
    /// backend that knows its own representation (a run is single-backend, so a
    /// mismatch is a programming error that cannot occur through the public API).
    pub fn downcast_ref<R: RelationData>(&self) -> &R {
        self.inner
            .as_any()
            .downcast_ref::<R>()
            .expect("relation runtime backend mismatch")
    }
}

impl Display for Relation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl Debug for Relation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.inner)
    }
}

/// The set of columns a plan's expressions may name at a given point, with each
/// column's type. Unlike a [`TableSchema`], this does not exist at runtime but
/// is only used during static analysis. Unlike a backend's physical schema,
/// it says nothing about keys or field order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationType {
    fields: HashMap<String, ScalarType>,
}

impl Display for RelationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Sort by field name so the output is deterministic (`HashMap`
        // iteration order is not).
        let mut fields: Vec<_> = self.fields.iter().collect();
        fields.sort_by_key(|(name, _)| *name);
        write!(f, "{{")?;
        let mut iter = fields.into_iter();
        if let Some((name, scalar_type)) = iter.next() {
            write!(f, "{name}: {scalar_type}")?;
            for (name, scalar_type) in iter {
                write!(f, ", {name}: {scalar_type}")?;
            }
        }
        write!(f, "}}")
    }
}

impl RelationType {
    // TODO: Maybe the relation type should be position-aware and allow for
    // duplicated columns sharing the same name..
    pub fn join(self, other: Self) -> Self {
        // We start with other to have duplicate fields' types be taken from self.
        let mut fields = other.fields;
        fields.extend(self.fields);
        Self { fields }
    }
    pub fn pick<T: AsRef<str>>(mut self, fields: impl IntoIterator<Item = T>) -> Self {
        let fields = fields
            .into_iter()
            .filter_map(|name| self.fields.remove_entry(name.as_ref()))
            .collect();
        Self { fields }
    }
    pub fn into_tuple_vars(self) -> HashMap<String, ScalarType> {
        self.fields
    }
    pub fn field_type(&self, name: &str) -> Option<&ScalarType> {
        self.fields.get(name)
    }
    pub fn intersect<'a>(&'a self, other: &Self) -> impl Iterator<Item = (&'a String, ScalarType)> {
        self.fields.iter().filter_map(|(name, self_scalar_type)| {
            // No type checking here, that is, we don't check if
            // `self_scalar_type` and `other_scalar_type` are the compatible.
            other
                .fields
                .get(name)
                .map(|other_scalar_type| (name, *other_scalar_type))
        })
    }
}

impl<'a> IntoIterator for &'a RelationType {
    type Item = (&'a String, &'a ScalarType);
    type IntoIter = std::collections::hash_map::Iter<'a, String, ScalarType>;

    fn into_iter(self) -> Self::IntoIter {
        self.fields.iter()
    }
}

impl<T, S> PartialEq<T> for RelationType
where
    // Is there a way to avoid the clone here?
    T: ExactSizeIterator<Item = S> + Clone,
    S: AsRef<str>,
{
    fn eq(&self, iter: &T) -> bool {
        let iter = iter.clone(); // Should be cheap, as it is an iterator.
        if self.fields.len() != iter.len() {
            return false;
        }
        for name in iter {
            if !self.fields.contains_key(name.as_ref()) {
                return false;
            }
        }
        true
    }
}

impl<'a> FromIterator<(&'a String, ScalarType)> for RelationType {
    fn from_iter<T: IntoIterator<Item = (&'a String, ScalarType)>>(iter: T) -> Self {
        let fields = iter
            .into_iter()
            .map(|(name, scalar_type)| (name.clone(), scalar_type))
            .collect();
        Self { fields }
    }
}

impl From<&TableSchema> for RelationType {
    fn from(value: &TableSchema) -> Self {
        let fields = value
            .columns()
            .iter()
            .map(|column| (column.name().to_string(), column.scalar_type()))
            .collect();
        Self { fields }
    }
}
