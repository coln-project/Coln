// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module expresses the different schema views according to coln-compiler,
//! coln-store, and coln-query in code.

use either::Either;

use crate::ir::{self, Path};
use std::{marker::PhantomData, ops::Range};

/// Typestate marker whether the [`ColnSchema`] represents a base table
/// and, therefore, has (implicit) row ids.
#[derive(Debug, Clone)]
pub struct BaseTable(());
/// Typestate marker whether the [`ColnSchema`] represents a derived view
/// and, therefore, does _not_ have (implicit) row ids.
#[derive(Debug, Clone)]
pub struct DerivedView(());

#[derive(Debug, Clone)]
pub struct ColnSchema<Marker> {
    /// Indicates whether this table schema is for a base table (by using the
    /// [`BaseTable`] marker) or a derived view (by using the [`DerivedView`]
    /// marker).
    marker: PhantomData<Marker>,
    /// The table's unique identifier/name.
    name: ir::Path,
    /// Fields of the table in their physical order from the perspective
    /// of the _compiler_. The columns _do not_ include the implicit row id.
    cols_compiler: CompilerCols,
    /// Fields of the table in their physical order from the perspective
    /// of the _storage engine_.
    cols_store: StoreEngineCols,
    /// Fields of the table in their physical order from the perspective
    /// of the _query engine_.
    cols_query: QueryEngineCols,
    /// The list of (possibly compound) primary keys into the table, specified
    /// as indices into the [compiler view](Self::cols_compiler).
    primary_keys: Vec<Vec<CompilerColIdx>>,
}

impl<Marker> ColnSchema<Marker> {
    /// The name of the base table.
    pub fn name(&self) -> &ir::Path {
        &self.name
    }
    pub fn compiler_cols(&self) -> &CompilerCols {
        &self.cols_compiler
    }
    /// Returns [`None`] if `idx` is an index for an implicit row id.
    pub fn get_compiler_col(&self, idx: CompilerColIdx) -> Option<&CompilerCol> {
        match idx {
            CompilerColIdx::RowId => None,
            CompilerColIdx::Column(idx) => Some(&self.cols_compiler.0[idx as usize]),
        }
    }
    pub fn storage_cols(&self) -> &StoreEngineCols {
        &self.cols_store
    }
    pub fn get_storage_col(&self, idx: StoreEngineColIdx) -> &StoreEngineCol {
        &self.cols_store.0[idx.0]
    }
    pub fn query_cols(&self) -> &QueryEngineCols {
        &self.cols_query
    }
    pub fn get_query_col(&self, idx: QueryEngineColIdx) -> &QueryEngineCol {
        &self.cols_query.0[idx.0]
    }
    /// The list of (compound) primary key(s), given as indexes into the
    /// compiler's column view.
    ///
    /// Hint: Compiler indexes can be converted into other views using the
    /// [`resolve_*`](Self::resolve_query_cols) methods.
    pub fn primary_keys(&self) -> &Vec<Vec<CompilerColIdx>> {
        &self.primary_keys
    }
    fn new(
        name: &ir::Path,
        columns: &[ir::ColumnEntry],
        primary_keys: &Option<Vec<ir::ColumnIdx>>,
        with_implicit_row_id: bool,
    ) -> Self {
        let cols_compiler =
            CompilerCols::from(columns.iter().map(|col| (&col.path, &col.col_type)));
        let cols_store = StoreEngineCols::from(cols_compiler.0.as_slice(), with_implicit_row_id);
        let cols_query = QueryEngineCols::from(cols_store.0.as_slice());
        let primary_key = primary_keys
            .as_ref()
            // Currently, `null` in JSON becomes the empty vector.
            .map_or(Vec::new(), |compound_primary_key| {
                compound_primary_key
                    .iter()
                    .map(|primary_key_column| CompilerColIdx::Column(*primary_key_column))
                    .collect::<Vec<_>>()
            });
        // Currently, the compiler supports only a single primary key.
        let primary_keys = vec![primary_key];
        ColnSchema {
            name: name.clone(),
            cols_compiler,
            cols_store,
            cols_query,
            primary_keys,
            marker: PhantomData,
        }
    }
}

impl ColnSchema<BaseTable> {
    pub fn resolve_query_cols(
        &self,
        idx: CompilerColIdx,
    ) -> impl ExactSizeIterator<Item = (QueryEngineColIdx, &QueryEngineCol)> {
        let query_range = match idx {
            CompilerColIdx::RowId => 0..StoreEngineCols::ROW_ID_COLS,
            CompilerColIdx::Column(target_idx) => {
                compiler_idx_to_query_idx(self.compiler_cols(), target_idx as usize, true)
            }
        };
        query_range.map(|query_idx| (QueryEngineColIdx(query_idx), &self.cols_query.0[query_idx]))
    }
}

impl ColnSchema<DerivedView> {
    pub fn resolve_query_cols(
        &self,
        idx: ir::ColumnIdx,
    ) -> impl ExactSizeIterator<Item = (QueryEngineColIdx, &QueryEngineCol)> {
        let query_range = compiler_idx_to_query_idx(self.compiler_cols(), idx as usize, false);
        query_range.map(|query_idx| (QueryEngineColIdx(query_idx), &self.cols_query.0[query_idx]))
    }
}

pub trait ResolveCompilerIdxToQueryView {
    /// Given a [`CompilerColIdx`] from the FLIR, indexing into the columns of
    /// the compiler view, what are the corresponding column(s) according to the
    /// query engine's view? This translation is necessary because row ids
    /// flatten into two columns from the perspective of the query engine,
    /// hence, a (compiler) index resolving to a row id column can result in
    /// two columns. A (compiler) index to a non row id column results in
    /// exactly one column.
    fn resolve_query_cols(
        &self,
        idx: CompilerColIdx,
    ) -> Option<impl ExactSizeIterator<Item = (QueryEngineColIdx, &QueryEngineCol)>>;
}

pub enum ColnSchemaWrapper<'a> {
    BaseTable(&'a ColnSchema<BaseTable>),
    DerivedView(&'a ColnSchema<DerivedView>),
}

impl ResolveCompilerIdxToQueryView for ColnSchemaWrapper<'_> {
    fn resolve_query_cols(
        &self,
        idx: CompilerColIdx,
    ) -> Option<impl ExactSizeIterator<Item = (QueryEngineColIdx, &QueryEngineCol)>> {
        match self {
            ColnSchemaWrapper::BaseTable(base) => Some(Either::Left(base.resolve_query_cols(idx))),
            ColnSchemaWrapper::DerivedView(view) => match idx {
                CompilerColIdx::RowId => None,
                CompilerColIdx::Column(idx) => Some(Either::Right(view.resolve_query_cols(idx))),
            },
        }
    }
}

impl From<&ir::TableEntry> for Option<ColnSchema<BaseTable>> {
    fn from(value: &ir::TableEntry) -> Self {
        let schema = &value.table;
        if !matches!(schema.entity_variant, ir::EntityVariant::Table) {
            return None; // Only base tables allowed.
        }
        Some(ColnSchema::new(
            &value.path,
            &schema.columns,
            &schema.primary_key,
            true,
        ))
    }
}

impl From<&ir::TableEntry> for Option<ColnSchema<DerivedView>> {
    fn from(value: &ir::TableEntry) -> Self {
        let schema = &value.table;
        if !matches!(schema.entity_variant, ir::EntityVariant::View { .. }) {
            return None; // Only derived views allowed.
        }
        Some(ColnSchema::new(
            &value.path,
            &schema.columns,
            &schema.primary_key,
            false,
        ))
    }
}

pub struct RuleVars {
    cols_compiler: CompilerCols,
    cols_query: QueryEngineCols,
}

impl RuleVars {
    pub fn new(vars: &[(ir::ColName, ir::ColType)]) -> Self {
        let cols_compiler = CompilerCols::from(vars.iter().map(|var| (&var.0, &var.1)));
        // The storage view must be computed to obtain the query view
        // at the moment.
        let cols_store = StoreEngineCols::from(cols_compiler.0.as_slice(), false);
        let cols_query = QueryEngineCols::from(cols_store.0.as_slice());
        Self {
            cols_compiler,
            cols_query,
        }
    }
    pub fn query_cols(&self) -> &QueryEngineCols {
        &self.cols_query
    }
    /// Given a [`ir::VarIdx`] from the FLIR, indexing into the vars array of
    /// a rule, what are the corresponding column(s)/variables according to the
    /// query engine's view? This translation is necessary because row ids
    /// flatten into two columns from the perspective of the query engine,
    /// hence, a (compiler) index resolving to a row id column can result in
    /// two columns/variables.
    pub fn resolve_query_cols(&self, idx: ir::VarIdx) -> impl Iterator<Item = &QueryEngineCol> {
        let range = compiler_idx_to_query_idx(&self.cols_compiler, idx as usize, false);
        self.cols_query.0[range].iter()
    }
}

fn compiler_idx_to_query_idx(
    compiler_cols: &CompilerCols,
    target_idx: usize,
    account_for_implicit_row_id: bool,
) -> Range<usize> {
    assert!(
        target_idx < compiler_cols.0.len(),
        "Compiler idx out of bounds"
    );
    // We account for the implicit row id columns by offsetting.
    let mut query_idx = if account_for_implicit_row_id {
        StoreEngineCols::ROW_ID_COLS
    } else {
        0
    };
    let mut iter = compiler_cols.0.iter().enumerate();
    let target_col = loop {
        let (idx, col) = iter.next().unwrap();
        if idx >= target_idx {
            break col;
        }
        match &col.ty {
            // A column of a native scalar type also takes just one column.
            ir::ColType::BuiltinTy { builtin_ty: _ } => query_idx += 1,
            // A row id flattens into multiple columns in the query engine's
            // view, so we have to advance more columns.
            ir::ColType::RowId { path: _ } => query_idx += StoreEngineCols::ROW_ID_COLS,
        };
    };
    match &target_col.ty {
        ir::ColType::BuiltinTy { builtin_ty: _ } => query_idx..query_idx + 1,
        ir::ColType::RowId { path: _ } => query_idx..query_idx + StoreEngineCols::ROW_ID_COLS,
    }
}

// Scalar types.

/// Scalar types which are supported natively by both coln-store and coln-query.
#[derive(Clone, Copy, Debug)]
pub enum NativeScalarType {
    /// Signed 64-bit integer.
    Iint,
    /// Unsigned 64-bit integer.
    Uint,
    /// String.
    String,
    // Add more :)
}

impl From<ir::BuiltinTy> for NativeScalarType {
    fn from(value: ir::BuiltinTy) -> Self {
        match value {
            ir::BuiltinTy::BuiltinStr => NativeScalarType::String,
            ir::BuiltinTy::BuiltinInt => NativeScalarType::Iint,
            // So far, no builtin uint.
        }
    }
}

/// Scalar types which are supported by coln-store.
#[derive(Clone, Copy, Debug)]
pub enum StoreEngineScalarType {
    /// A row id becomes a pair of `(CommitHash, Counter)`.
    CommitHash,
    /// A row id becomes a pair of `(CommitHash, Counter)`.
    Counter,
    Native(NativeScalarType),
}

/// Scalar types which are supported by coln-query.
#[derive(Clone, Copy, Debug)]
pub enum QueryEngineScalarType {
    Native(NativeScalarType),
}

impl From<StoreEngineScalarType> for QueryEngineScalarType {
    fn from(value: StoreEngineScalarType) -> Self {
        match value {
            StoreEngineScalarType::CommitHash => {
                QueryEngineScalarType::Native(NativeScalarType::Uint)
            }
            StoreEngineScalarType::Counter => QueryEngineScalarType::Native(NativeScalarType::Uint),
            StoreEngineScalarType::Native(native) => QueryEngineScalarType::Native(native),
        }
    }
}

/// Generic column metadata representation.
#[derive(Debug, Clone)]
pub struct Col<T, R> {
    /// The column's name.
    name: ir::ColName,
    /// The column's (scalar) type.
    ty: T,
    /// If the column is (part of) a foreign key, this links the referenced table.
    references: R,
}

impl<T, R> Col<T, R> {
    pub fn name(&self) -> &ir::ColName {
        &self.name
    }
    /// The column's (scalar) type, in whichever engine's view `T` belongs to.
    pub fn ty(&self) -> &T {
        &self.ty
    }
}

/// Column metadata from the perspective of the compiler.
///
/// The compiler encodes foreign keys as part of the type of a column (see the
/// [`ir::ColType::RowId`] variant of [`ir::ColType`]).
/// Hence, `R` becomes the unit type and is not required in this case.
pub type CompilerCol = Col<ir::ColType, ()>;

#[derive(Copy, Clone, Debug)]
pub enum CompilerColIdx {
    /// A reference to the table's row id (the implicit primary key).
    RowId,
    /// A reference to a column is a (zero-indexed) column index.
    Column(ir::ColumnIdx),
}

impl CompilerColIdx {
    pub fn for_row_id() -> Self {
        CompilerColIdx::RowId
    }
}

impl From<ir::ColumnIdx> for CompilerColIdx {
    fn from(value: ir::ColumnIdx) -> Self {
        CompilerColIdx::Column(value)
    }
}

#[derive(Debug, Clone)]
pub struct CompilerCols(Vec<CompilerCol>);

impl CompilerCols {
    pub fn inner(&self) -> &[CompilerCol] {
        &self.0
    }
    fn from<'a>(ir_cols: impl IntoIterator<Item = (&'a ir::ColName, &'a ir::ColType)>) -> Self {
        CompilerCols(
            ir_cols
                .into_iter()
                // It's an one-to-one mapping from FLIR's JSON representation
                // to this intermediate representation.
                .map(|(name, col_type)| CompilerCol {
                    name: name.clone(),
                    ty: col_type.clone(),
                    // Foreign keys are encoded in the `ty` for a CompilerColumn.
                    // Hence, references becomes the unit type.
                    references: (),
                })
                .collect(),
        )
    }
}

pub type StoreEngineCol = Col<StoreEngineScalarType, Option<ir::Path>>;

#[derive(Copy, Clone, Debug)]
pub struct StoreEngineColIdx(usize);

#[derive(Debug, Clone)]
pub struct StoreEngineCols(Vec<StoreEngineCol>);

impl StoreEngineCols {
    /// To how many columns a row id expands to.
    pub const ROW_ID_COLS: usize = 2;
    /// The suffix of the hash column of a row id.
    pub const HASH_COL_SUFFIX: &'static str = "RowIdHash";
    /// The suffix of the counter column of a row id.
    pub const CTR_COL_SUFFIX: &'static str = "RowIdCtr";

    /// From the perspective of coln-store, every base table has two implicitly
    /// defined columns: The commit hash from the transaction which created the
    /// row and a counter value, rendering the hash-counter-pair unique among
    /// all insertions of a transaction. Coln-store assigns these counters.
    fn implicit_row_id_cols() -> [StoreEngineCol; Self::ROW_ID_COLS] {
        [
            StoreEngineCol {
                name: Path::from(Self::HASH_COL_SUFFIX),
                ty: StoreEngineScalarType::CommitHash,
                references: None,
            },
            StoreEngineCol {
                name: Path::from(Self::CTR_COL_SUFFIX),
                ty: StoreEngineScalarType::Counter,
                references: None,
            },
        ]
    }
    fn foreign_key_cols(
        name: &ir::ColName,
        foreign_entity: &Path,
    ) -> [StoreEngineCol; Self::ROW_ID_COLS] {
        [
            StoreEngineCol {
                name: name.clone().append(Self::HASH_COL_SUFFIX),
                ty: StoreEngineScalarType::CommitHash,
                references: Some(foreign_entity.clone()),
            },
            StoreEngineCol {
                name: name.clone().append(Self::CTR_COL_SUFFIX),
                ty: StoreEngineScalarType::Counter,
                references: Some(foreign_entity.clone()),
            },
        ]
    }
    fn from(compiler_cols: &[CompilerCol], with_implicit_row_id: bool) -> Self {
        let implicit = with_implicit_row_id
            .then(StoreEngineCols::implicit_row_id_cols)
            .into_iter()
            .flatten();
        let schema_cols = compiler_cols.iter().flat_map(|col| {
            let name = col.name.clone();
            let (first, second) = match &col.ty {
                ir::ColType::RowId { path } => {
                    let [hash_col, ctr_col] = StoreEngineCols::foreign_key_cols(&name, path);
                    (hash_col, Some(ctr_col))
                }
                ir::ColType::BuiltinTy { builtin_ty } => (
                    StoreEngineCol {
                        name,
                        ty: StoreEngineScalarType::Native(NativeScalarType::from(*builtin_ty)),
                        references: None,
                    },
                    None,
                ),
            };
            std::iter::once(first).chain(second)
        });
        StoreEngineCols(implicit.chain(schema_cols).collect())
    }
}

pub type QueryEngineCol = Col<QueryEngineScalarType, Option<ir::Path>>;

#[derive(Copy, Clone, Debug)]
pub struct QueryEngineColIdx(pub usize);

#[derive(Debug, Clone)]
pub struct QueryEngineCols(Vec<QueryEngineCol>);

impl QueryEngineCols {
    pub fn inner(&self) -> &[QueryEngineCol] {
        &self.0
    }
    fn from(store_engine_cols: &[StoreEngineCol]) -> Self {
        QueryEngineCols(
            store_engine_cols
                .iter()
                // It's a one-to-one mapping from the storage engine's schema
                // view to the query engine's schema view; only the scalar types
                // are different: The commit hash and counter become plain,
                // unsigned ints, each.
                .map(|col| QueryEngineCol {
                    name: col.name.clone(),
                    ty: QueryEngineScalarType::from(col.ty),
                    references: col.references.clone(),
                })
                .collect(),
        )
    }
}
