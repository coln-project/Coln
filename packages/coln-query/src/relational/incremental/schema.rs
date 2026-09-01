// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The DBSP backend's *physical* schema: how a relation's rows are actually laid
//! out for a circuit to compute on.
//!
//! A DBSP relation is an `OrdIndexedZSet<TupleKey, TupleValue>`, so every
//! relation is *keyed* — and every operator that changes the shape of a row has
//! to say what the key and the value of its output are. That is what
//! [`StreamSchema`] tracks, and why it lives here rather than beside the
//! backend-neutral [`TableSchema`]: "this relation is indexed by these columns"
//! is a statement about `OrdIndexedZSet`, not about a table. A table may declare
//! several candidate keys or none at all; [`from`](StreamSchema::from) is where
//! that becomes the one key DBSP indexes by.
//!
//! [`TupleSchema`]'s inactive-field bookkeeping ([`FieldInfo::active`]) is
//! physical for the same reason: a projection marks columns dropped without
//! rebuilding every tuple, and the actual compaction happens later, at a point
//! where the circuit needs coalesced rows (see
//! [`coalesce_helper`](super::operators::coalesce::coalesce_helper)). Positions
//! in a `Vec<ScalarTypedValue>` are the thing being tracked, so the names of a
//! relation's columns and their indexes only mean something together.

use super::super::relation::{Tuple, TupleValue};
use crate::{
    host::interpreter::InterpreterContext,
    relational::schema::{Column, TableSchema},
    scalarial::ScalarTypedValue,
};
use dbsp::{never_none, never_roaring_filter};
use std::{
    collections::HashSet,
    fmt::{self, Debug, Display},
};

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
pub struct TupleKey {
    pub data: Vec<ScalarTypedValue>,
}

never_none!(TupleKey);
never_roaring_filter!(TupleKey);

impl<T: Into<ScalarTypedValue>> FromIterator<T> for TupleKey {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            data: iter.into_iter().map(|v| v.into()).collect(),
        }
    }
}

impl Tuple for TupleKey {
    fn data_at(&self, index: usize) -> &ScalarTypedValue {
        &self.data[index]
    }
    fn data(&self) -> impl Iterator<Item = &ScalarTypedValue> {
        self.data.iter()
    }
}

impl Display for TupleKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.data_to_string())
    }
}

pub struct SchemaTuple<'a, T> {
    schema: &'a TupleSchema,
    tuple: &'a T,
}

impl<'a, T: Tuple> SchemaTuple<'a, T> {
    pub fn new(schema: &'a TupleSchema, tuple: &'a T) -> Self {
        Self { schema, tuple }
    }
    pub fn fields(&self) -> impl Iterator<Item = &'a ScalarTypedValue> {
        self.schema
            .active_fields()
            .map(|(index, info)| self.tuple.data_at(index))
    }
    pub fn all_fields(&self) -> impl Iterator<Item = &'a ScalarTypedValue> {
        self.schema
            .all_fields()
            .map(|(index, _info)| self.tuple.data_at(index))
    }
    pub fn named_fields(
        &self,
        alias: &Option<String>,
    ) -> impl Iterator<Item = (String, ScalarTypedValue)> {
        self.schema
            .active_fields()
            .map(|(index, info)| (info.name(alias), self.tuple.data_at(index).clone()))
    }
    pub fn coalesce(&self) -> impl Iterator<Item = ScalarTypedValue> {
        self.schema
            .active_fields()
            .map(|(index, info)| self.tuple.data_at(index).clone())
    }
    pub fn pick(&self, fields: &[String]) -> impl Iterator<Item = ScalarTypedValue> {
        self.schema.active_fields().filter_map(|(index, info)| {
            if fields.contains(&info.name) {
                Some(self.tuple.data_at(index).clone())
            } else {
                None
            }
        })
    }
    pub fn join(&self, other: &Self) -> impl Iterator<Item = ScalarTypedValue> {
        self.fields().chain(other.fields()).cloned()
    }
}

impl Debug for SchemaTuple<'_, TupleValue> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.schema.active_fields().map(|(index, info)| {
                format!("{}: {}", info.name(&None), self.tuple.data_at(index))
            }))
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldInfo {
    /// The field's name.
    name: String,
    /// Whether the field is active, that is, not eliminated by, e.g.,
    /// a projection.
    active: bool,
    // Maybe add type information here, too.
}

impl FieldInfo {
    fn new(name: String) -> Self {
        Self { name, active: true }
    }
    fn name(&self, alias: &Option<String>) -> String {
        let name = alias
            .as_ref()
            .map(|alias| format!("{}.{}", alias, self.name))
            .unwrap_or_else(|| self.name.clone());
        if self.active {
            name
        } else {
            format!("{name}*")
        }
    }
}

type Index = usize;

#[derive(Clone, PartialEq, Eq)]
pub struct TupleSchema {
    fields: Vec<FieldInfo>,
}

impl TupleSchema {
    pub fn new<T: Into<String>>(fields: impl IntoIterator<Item = T>) -> Self {
        Self {
            fields: fields
                .into_iter()
                .map(|name| FieldInfo::new(name.into()))
                .collect(),
        }
    }
    pub fn empty() -> Self {
        Self { fields: vec![] }
    }
    /// Only the active fields are included in the count.
    pub fn len(&self) -> usize {
        self.fields.iter().filter(|info| info.active).count()
    }
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
    /// Includes the active and inactive fields in the count.
    pub fn full_len(&self) -> usize {
        self.fields.len()
    }
    fn is_coalesced(&self) -> bool {
        !self.fields.iter().any(|info| !info.active)
    }
    fn coalesce(&self) -> Self {
        self.fields
            .iter()
            .filter(|info| info.active)
            .cloned()
            .collect()
    }
    fn active_fields(&self) -> impl Iterator<Item = (Index, &FieldInfo)> {
        self.fields
            .iter()
            .enumerate()
            .filter(|(_index, info)| info.active)
    }
    fn all_fields(&self) -> impl Iterator<Item = (Index, &FieldInfo)> {
        self.fields.iter().enumerate()
    }
    pub fn field_names(&self, alias: &Option<String>) -> impl Iterator<Item = String> {
        self.active_fields().map(|(_index, info)| info.name(alias))
    }
    pub fn all_field_names(&self, alias: &Option<String>) -> impl Iterator<Item = String> {
        self.all_fields().map(|(_index, info)| info.name(alias))
    }
    fn select(&self) -> Self {
        self.clone()
    }
    /// We mark all fields as inactive, that is, we forget about them.
    fn forget(&self) -> Self {
        self.fields
            .iter()
            .map(|info| FieldInfo {
                name: info.name.clone(),
                active: false,
            })
            .collect()
    }
    /// In contrast to the `project` method, this method does not remove fields
    /// from the schema but marks them as inactive, thereby not coalescing the
    /// schema and the order of fields. Optionally, you can rename a field by
    /// providing an alias/new name/target name as a second element.
    fn pick(&self, fields: &Vec<(&String, Option<&String>)>) -> Self {
        // For keeping track of duplicated field names.
        let mut active = HashSet::with_capacity(fields.len());
        // Don't use active_fields() here because the tuple is not coalesced
        // but we only allow picking from the set of active fields though.
        self.all_fields()
            .map(|(_index, info)| {
                // We do not reactivate already inactive fields.
                if !info.active {
                    return info.clone();
                }
                if let Some((source_name, target_name)) =
                    fields.iter().find(|field| *field.0 == info.name)
                {
                    let name = target_name.cloned().unwrap_or_else(|| info.name.clone());
                    if !active.contains(&name) {
                        active.insert(name.clone());
                        return FieldInfo::new(name); // Field is active by constructor.
                    } else {
                        // We have a duplicated field name, so we mark it as inactive.
                        return FieldInfo {
                            name,
                            active: false,
                        };
                    }
                }
                // Field is not in the list of fields to pick, so we mark it as inactive.
                FieldInfo {
                    name: info.name.clone(),
                    active: false,
                }
            })
            .collect()
    }
    /// In case of a full projection, we coalesce the schema and remove inactive
    /// fields. The order is also redefined according to the projection.
    fn project(&self, fields: Vec<String>) -> Self {
        fields.into_iter().collect()
    }
    fn join(&self, other: &Self) -> Self {
        let self_active_field_table: HashSet<&String> =
            self.active_fields().map(|(_, info)| &info.name).collect();
        // We mark every active field of `other` as inactive if it is
        // shadowed by an active field of `self` with the same name.
        let other_fields = other.active_fields().map(|(_, info)| {
            let mut info = info.clone();
            if self_active_field_table.contains(&info.name) {
                info.active = false;
            }
            info
        });
        self.active_fields()
            .map(|(_index, info)| info.clone())
            .chain(other_fields)
            .collect()
    }
    fn fields_to_string<'a>(
        &self,
        fields: impl Iterator<Item = (Index, &'a FieldInfo)>,
        with_extra: bool,
    ) -> String {
        let fields = fields
            .map(|(_, info)| info.name(&None))
            .collect::<Vec<_>>()
            .join("|");
        format!("|{fields}|")
    }
}

impl FromIterator<FieldInfo> for TupleSchema {
    fn from_iter<I: IntoIterator<Item = FieldInfo>>(iter: I) -> Self {
        Self {
            fields: iter.into_iter().collect(),
        }
    }
}

impl FromIterator<String> for TupleSchema {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        Self {
            fields: iter.into_iter().map(FieldInfo::new).collect(),
        }
    }
}

impl Debug for TupleSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.fields_to_string(self.all_fields(), true))
    }
}

impl Display for TupleSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.fields_to_string(self.active_fields(), false))
    }
}

/// The physical schema of a keyed DBSP relation: which fields make up its
/// `TupleKey`, which make up its `TupleValue`, and at which index each sits.
///
/// Unlike a [`RelationType`](crate::relational::relation::RelationType),
/// this exists at runtime: An operator needs it to read a field of the row
/// in hand by name.
/// Unlike a [`TableSchema`], it describes one *stream* rather than one table:
/// every operator that reshapes rows derives a new one (see
/// [`select`](Self::select), [`join`](Self::join), …), so its
/// [`name`](Self::name) is a transformation trace rather than an identity.
#[derive(Clone, Debug)]
pub struct StreamSchema {
    /// Not a real name to reference the relation but more like a transformation
    /// trace. Real names are handled by variable names.
    pub name: String,
    pub key: TupleSchema,
    pub tuple: TupleSchema,
}

impl StreamSchema {
    pub fn is_coalesced(&self) -> bool {
        self.key.is_coalesced() && self.tuple.is_coalesced()
    }
    pub fn coalesce(&self) -> Self {
        Self {
            name: format!("[{}-coalesced]", self.name),
            key: self.key.coalesce(),
            tuple: self.tuple.coalesce(),
        }
    }
    /// Just clones the current schema, as selections do not alter the schema.
    pub fn select(&self) -> Self {
        Self {
            name: format!("[{}-selected]", self.name),
            key: self.key.clone(),
            tuple: self.tuple.clone(),
        }
    }
    pub fn pick(&self, fields: &Vec<(&String, Option<&String>)>) -> Self {
        Self {
            name: format!("[{}-picked]", self.name),
            // To keep the `ProjectionExpr`'s semantics consistent,
            // we erase the key here, too, as we do for the full projection below.
            key: self.key.forget(),
            tuple: self.tuple.pick(fields),
        }
    }
    pub fn project(&self, fields: Vec<String>) -> Self {
        Self {
            name: format!("[{}-projected]", self.name),
            key: TupleSchema::empty(),
            tuple: self.tuple.project(fields),
        }
    }
    pub fn join(&self, other: &Self, key_fields: impl IntoIterator<Item = String>) -> Self {
        Self {
            name: format!("[{}-{}-joined]", self.name, other.name),
            key: key_fields.into_iter().collect(),
            tuple: self.tuple.join(&other.tuple),
        }
    }
    pub fn anti_join(&self, other: &Self, key_fields: impl IntoIterator<Item = String>) -> Self {
        // We do not need to store the key in the schema, as it is not used
        // in the anti-join.
        Self {
            name: format!("{}-{}-anti-joined", self.name, other.name),
            key: key_fields.into_iter().collect(),
            tuple: self.tuple.clone(),
        }
    }
}

impl Display for StreamSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<relation {}>", self.name)
    }
}

impl PartialEq for StreamSchema {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.tuple == other.tuple
    }
}

impl Eq for StreamSchema {}

/// Where a table's key(s) become *the* key a DBSP circuit indexes by.
///
/// A [`TableSchema`] may declare several candidate keys, or none; an
/// `OrdIndexedZSet` has exactly one. We take the first declared key. For a
/// FLIR base table that is the row id, the only key guaranteed to be unique,
/// and fall back to the empty key, which indexes every row under the same
/// (empty) key, for a relation that declares none.
impl From<&TableSchema> for StreamSchema {
    fn from(table: &TableSchema) -> Self {
        let key = table
            .primary_keys()
            .next()
            .map(|primary_key| TupleSchema::new(primary_key.map(Column::name)))
            .unwrap_or_else(TupleSchema::empty);
        Self {
            name: table.name().to_string(),
            key,
            tuple: TupleSchema::new(table.columns().iter().map(Column::name)),
        }
    }
}

/// Bind the fields of one physical row as scalar variables, so that a host
/// scalar fragment (a selection condition, a projection attribute, a join key)
/// can name a column of the row currently being processed.
///
/// An extension of the host's [`InterpreterContext`] rather than a method on it:
/// the host offers the variable map, but only a backend knows how a row of its
/// own is laid out, which is exactly what a [`TupleSchema`] describes.
pub trait DbspTupleContext {
    fn extend_tuple_ctx<T: Tuple>(
        &mut self,
        alias: &Option<String>,
        schema: &TupleSchema,
        tuple: &T,
    );
}

impl DbspTupleContext for InterpreterContext<'_> {
    fn extend_tuple_ctx<T: Tuple>(
        &mut self,
        alias: &Option<String>,
        schema: &TupleSchema,
        tuple: &T,
    ) {
        self.tuple_vars
            .extend(SchemaTuple::new(schema, tuple).named_fields(alias));
    }
}
