// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module expresses the different schema views according to coln-compiler,
//! coln-store, and coln-query in code.

use crate::{
    engine::{
        packed::{PackedRowIdType, StoreScalarType},
        query::QueryScalarType,
    },
    ir::{self, Path},
};
use either::Either;
use serde::{Deserialize, Serialize};
use std::{marker::PhantomData, ops::Range};

/// Typestate marker whether the [`ColnSchema`] represents a base table
/// and, therefore, has (implicit) row ids.
#[derive(Debug, Clone)]
pub struct BaseTable(());
/// Typestate marker whether the [`ColnSchema`] represents a derived view
/// and, therefore, does _not_ have (implicit) row ids.
#[derive(Debug, Clone)]
pub struct DerivedView(());

// @Leo, I think best to describe this with the [`ir::Schema`]? We kind of have
// two representations for a schema right now.
// @Vincent: I see what you mean but I think we want a layer on top of that.
// Anything from ir::* is not within our power and can change anytime
// because coln-compiler has authority over it. And the shape in ir::* is rarely
// the shape I want, see this module here.
//
// Also we probably need to store a map somewhere from table name to their schema,
// so the `name` field is redundant?
// @Vincent: If we have this, yes, why not.
//
// Finally the map I mentioned above for schemas, do we want to store that in
// bouncer/store/query? Seems a bit wasteful to store three copies, but I guess
// all of them would need to use the schema.... The alternative is to store it
// in coln-store and provide APIs ask for the schema.
// @Vincent: Yes, I'm all in to lift this component and make it available as
// our central schema provider somewhere. The question is where exactly?
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
        let cols_store = StoreEngineCols::from(cols_compiler.0.as_slice());
        let cols_query = QueryEngineCols::from(cols_compiler.0.as_slice(), with_implicit_row_id);
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
            CompilerColIdx::RowId => 0..QueryEngineCols::ROW_ID_COLS,
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
        let cols_query = QueryEngineCols::from(cols_compiler.0.as_slice(), false);
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
        QueryEngineCols::ROW_ID_COLS
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
            ir::ColType::RowId { path: _ } => query_idx += QueryEngineCols::ROW_ID_COLS,
        };
    };
    match &target_col.ty {
        ir::ColType::BuiltinTy { builtin_ty: _ } => query_idx..query_idx + 1,
        ir::ColType::RowId { path: _ } => query_idx..query_idx + QueryEngineCols::ROW_ID_COLS,
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
    pub fn references(&self) -> &R {
        &self.references
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

pub type StoreEngineCol = Col<StoreScalarType, Option<ir::Path>>;

#[derive(Copy, Clone, Debug)]
pub struct StoreEngineColIdx(usize);

#[derive(Debug, Clone)]
pub struct StoreEngineCols(Vec<StoreEngineCol>);

impl StoreEngineCols {
    /// From the perspective of coln-store, every base table has one implicitly
    /// defined first column: the row id of the row. Unlike in the query
    /// engine's view, it is a single column of a packed row id type rather than
    /// a (commit hash, counter) pair.
    fn implicit_row_id_col() -> StoreEngineCol {
        StoreEngineCol {
            name: Path::from("RowId"),
            ty: StoreScalarType::RowId(PackedRowIdType::default()),
            references: None,
        }
    }
    fn foreign_key_col(name: &ir::ColName, foreign_entity: &Path) -> StoreEngineCol {
        StoreEngineCol {
            name: name.clone(),
            ty: StoreScalarType::RowId(PackedRowIdType::default()),
            references: Some(foreign_entity.clone()),
        }
    }
    fn from(compiler_cols: &[CompilerCol]) -> Self {
        StoreEngineCols(
            std::iter::once(Self::implicit_row_id_col())
                .chain(compiler_cols.iter().map(|col| match &col.ty {
                    ir::ColType::RowId { path } => Self::foreign_key_col(&col.name, path),
                    ir::ColType::BuiltinTy { builtin_ty } => StoreEngineCol {
                        name: col.name.clone(),
                        ty: StoreScalarType::from(builtin_ty),
                        references: None,
                    },
                }))
                .collect(),
        )
    }
}

pub type QueryEngineCol = Col<QueryScalarType, Option<ir::Path>>;

#[derive(Copy, Clone, Debug)]
pub struct QueryEngineColIdx(pub usize);

#[derive(Debug, Clone)]
pub struct QueryEngineCols(Vec<QueryEngineCol>);

impl QueryEngineCols {
    /// To how many columns a row id expands to.
    pub const ROW_ID_COLS: usize = 2;
    /// The suffix of the hash column of a row id.
    pub const HASH_COL_SUFFIX: &'static str = "RowIdHash";
    /// The suffix of the counter column of a row id.
    pub const CTR_COL_SUFFIX: &'static str = "RowIdCtr";

    /// From the perspective of coln-query, a row id is flattened into two
    /// columns: the commit hash of the creating transaction and a counter,
    /// which together are unique among all insertions.
    fn implicit_row_id_cols() -> [QueryEngineCol; Self::ROW_ID_COLS] {
        [
            QueryEngineCol {
                name: ir::Path::from(Self::HASH_COL_SUFFIX),
                ty: QueryScalarType::U64(()),
                references: None,
            },
            QueryEngineCol {
                name: ir::Path::from(Self::CTR_COL_SUFFIX),
                ty: QueryScalarType::U64(()),
                references: None,
            },
        ]
    }
    fn foreign_key_cols(
        name: &ir::ColName,
        foreign_entity: &ir::Path,
    ) -> [QueryEngineCol; Self::ROW_ID_COLS] {
        [
            QueryEngineCol {
                name: name.clone().append(Self::HASH_COL_SUFFIX),
                ty: QueryScalarType::U64(()),
                references: Some(foreign_entity.clone()),
            },
            QueryEngineCol {
                name: name.clone().append(Self::CTR_COL_SUFFIX),
                ty: QueryScalarType::U64(()),
                references: Some(foreign_entity.clone()),
            },
        ]
    }
    pub fn inner(&self) -> &[QueryEngineCol] {
        &self.0
    }
    fn from(compiler_cols: &[CompilerCol], with_implicit_row_id: bool) -> Self {
        let row_id_cols = with_implicit_row_id
            .then(Self::implicit_row_id_cols)
            .into_iter()
            .flatten();
        let schema_cols = compiler_cols.iter().flat_map(|col| {
            let name = col.name.clone();
            let (first, second) = match &col.ty {
                ir::ColType::RowId { path } => {
                    let [hash_col, ctr_col] = QueryEngineCols::foreign_key_cols(&name, path);
                    (hash_col, Some(ctr_col))
                }
                ir::ColType::BuiltinTy { builtin_ty } => {
                    let col = QueryEngineCol {
                        name,
                        ty: QueryScalarType::from(builtin_ty),
                        references: None,
                    };
                    (col, None)
                }
            };
            std::iter::once(first).chain(second)
        });
        QueryEngineCols(row_id_cols.chain(schema_cols).collect())
    }
}

/// A Coln theory source file contains theory definitions and (multiple) realm
/// definitions. Each realm corresponds will be compiled to one IR file,
/// this struct stores which realm the IR is referring to. It is stored as
/// literal string and uninterpreted in the root commit of the store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColnDef {
    pub theory: String,
    pub realm: String,
}

impl ColnDef {
    pub fn new(theory: String, realm: String) -> Self {
        Self { theory, realm }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int() -> ir::ColType {
        ir::ColType::BuiltinTy {
            builtin_ty: ir::BuiltinTy::BuiltinInt,
        }
    }

    fn row_id(target: &str) -> ir::ColType {
        ir::ColType::RowId {
            path: Path::from(target),
        }
    }

    /// Columns `a: int, b: RowId(Other), c: int`.
    fn sample_vars() -> Vec<(ir::ColName, ir::ColType)> {
        vec![
            (Path::from("a"), int()),
            (Path::from("b"), row_id("Other")),
            (Path::from("c"), int()),
        ]
    }

    fn names(cols: &QueryEngineCols) -> Vec<&str> {
        cols.inner().iter().map(|c| c.name().as_ref()).collect()
    }

    #[test]
    fn query_cols_flatten_row_ids_without_implicit() {
        let vars = sample_vars();
        let compiler = CompilerCols::from(vars.iter().map(|v| (&v.0, &v.1)));
        let query = QueryEngineCols::from(compiler.0.as_slice(), false);
        assert_eq!(names(&query), ["a", "b.RowIdHash", "b.RowIdCtr", "c"]);
        assert!(matches!(query.inner()[1].ty(), QueryScalarType::U64(())));
        assert!(matches!(query.inner()[2].ty(), QueryScalarType::U64(())));
        assert_eq!(
            query.inner()[1].references().as_ref().unwrap(),
            &Path::from("Other")
        );
        assert!(query.inner()[0].references().is_none());
    }

    #[test]
    fn query_cols_prepend_implicit_row_id() {
        let vars = sample_vars();
        let compiler = CompilerCols::from(vars.iter().map(|v| (&v.0, &v.1)));
        let query = QueryEngineCols::from(compiler.0.as_slice(), true);
        assert_eq!(query.inner().len(), QueryEngineCols::ROW_ID_COLS + 4);
        assert_eq!(
            names(&query)[..QueryEngineCols::ROW_ID_COLS],
            [
                QueryEngineCols::HASH_COL_SUFFIX,
                QueryEngineCols::CTR_COL_SUFFIX
            ]
        );
    }

    #[test]
    fn store_cols_keep_row_ids_as_single_column() {
        let vars = sample_vars();
        let compiler = CompilerCols::from(vars.iter().map(|v| (&v.0, &v.1)));
        let store = StoreEngineCols::from(compiler.0.as_slice());
        // Implicit row id + the three compiler columns.
        assert_eq!(store.0.len(), 4);
        assert!(matches!(store.0[0].ty(), StoreScalarType::RowId(_)));
        assert!(matches!(store.0[1].ty(), StoreScalarType::I32(())));
        assert!(matches!(store.0[2].ty(), StoreScalarType::RowId(_)));
        assert_eq!(
            store.0[2].references().as_ref().unwrap(),
            &Path::from("Other")
        );
        assert!(matches!(store.0[3].ty(), StoreScalarType::I32(())));
    }

    #[test]
    fn compiler_idx_resolves_to_query_range() {
        let vars = sample_vars();
        let compiler = CompilerCols::from(vars.iter().map(|v| (&v.0, &v.1)));
        // Without implicit row id.
        assert_eq!(compiler_idx_to_query_idx(&compiler, 0, false), 0..1);
        assert_eq!(compiler_idx_to_query_idx(&compiler, 1, false), 1..3);
        assert_eq!(compiler_idx_to_query_idx(&compiler, 2, false), 3..4);
        // With implicit row id, everything shifts by `ROW_ID_COLS`.
        assert_eq!(compiler_idx_to_query_idx(&compiler, 0, true), 2..3);
        assert_eq!(compiler_idx_to_query_idx(&compiler, 1, true), 3..5);
        assert_eq!(compiler_idx_to_query_idx(&compiler, 2, true), 5..6);
    }

    #[test]
    fn rule_vars_resolve_query_cols() {
        let rule = RuleVars::new(&sample_vars());
        let resolved: Vec<_> = rule
            .resolve_query_cols(1)
            .map(|c| c.name().as_ref())
            .collect();
        assert_eq!(resolved, ["b.RowIdHash", "b.RowIdCtr"]);
    }
}
