use crate::ir::{self, Path};

pub struct BaseTableSchema {
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

impl BaseTableSchema {
    /// The name of the base table.
    pub fn name(&self) -> &ir::Path {
        &self.name
    }
    /// Returns [`None`] if `idx` is an index for an implicit row id.
    pub fn get_compiler_col(&self, idx: CompilerColIdx) -> Option<&CompilerCol> {
        match idx {
            CompilerColIdx::RowId => None,
            CompilerColIdx::Column(idx) => Some(&self.cols_compiler.0[idx as usize]),
        }
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
    /// Given a [`CompilerColIdx`] from the FLIR, indexing into the columns of
    /// the compiler view, what are the corresponding column(s) according to the
    /// query engine's view? This translation is necessary because row ids
    /// flatten into two columns from the perspective of the query engine,
    /// hence, a (compiler) index resolving to a row id column can result in
    /// two columns. A (compiler) index to a non row id column results in
    /// exactly one column.
    pub fn resolve_query_cols(&self, idx: CompilerColIdx) -> impl Iterator<Item = &QueryEngineCol> {
        let range = match idx {
            CompilerColIdx::RowId => 0..StoreEngineCols::ROW_ID_COLS,
            CompilerColIdx::Column(target_idx) => {
                assert!(
                    (target_idx as usize) < self.cols_compiler.0.len(),
                    "Compiler idx out of bounds"
                );
                // We account for the implicit row id columns by offsetting.
                let mut query_idx = StoreEngineCols::ROW_ID_COLS;
                let mut iter = self.cols_compiler.0.iter().enumerate();
                let target_col = loop {
                    let (idx, col) = iter.next().unwrap();
                    if idx >= target_idx as usize {
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
                    ir::ColType::RowId { path: _ } => query_idx..query_idx + 2,
                }
            }
        };
        self.cols_query.0[range].iter()
    }
    /// The list of (compound) primary key(s), given as indexes into the
    /// compiler's column view.
    ///
    /// Hint: Compiler indexes can be converted into other views using the
    /// [`resolve_*`](Self::resolve_query_cols) methods.
    pub fn primary_keys(&self) -> &Vec<Vec<CompilerColIdx>> {
        &self.primary_keys
    }
}

impl From<&ir::TableEntry> for Option<BaseTableSchema> {
    fn from(value: &ir::TableEntry) -> Self {
        let path = &value.path;
        let schema = &value.table;
        if !matches!(schema.entity_variant, ir::EntityVariant::Table) {
            return None; // Only base tables allowed.
        }
        let columns_compiler = CompilerCols::from(schema.columns.as_slice());
        let columns_store = StoreEngineCols::from(columns_compiler.0.as_slice());
        let columns_query = QueryEngineCols::from(columns_store.0.as_slice());
        let primary_key = schema
            .primary_key
            .as_ref()
            // Currently, `null` in JSON becomes the empty vector.
            .map_or(Vec::new(), |compound_primary_key| {
                compound_primary_key
                    .iter()
                    .map(|primary_key_column| {
                        schema
                            .columns
                            .iter()
                            .position(|column| column.path == *primary_key_column)
                            .map(|idx| CompilerColIdx::Column(idx as u64))
                            .unwrap_or_else(|| panic!("Primary key column {primary_key_column} not found in base table {path}"))
                    })
                    .collect::<Vec<_>>()
            });
        // Currently, the compiler supports only a single primary key.
        let primary_keys = vec![primary_key];
        Some(BaseTableSchema {
            name: path.clone(),
            cols_compiler: columns_compiler,
            cols_store: columns_store,
            cols_query: columns_query,
            primary_keys,
        })
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

pub struct CompilerCols(Vec<CompilerCol>);

pub type StoreEngineCol = Col<StoreEngineScalarType, Option<ir::Path>>;

#[derive(Copy, Clone, Debug)]
pub struct StoreEngineColIdx(usize);

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
}

pub type QueryEngineCol = Col<QueryEngineScalarType, Option<ir::Path>>;

#[derive(Copy, Clone, Debug)]
pub struct QueryEngineColIdx(usize);

pub struct QueryEngineCols(Vec<QueryEngineCol>);

impl QueryEngineCols {
    pub fn iter(&self) -> std::slice::Iter<'_, QueryEngineCol> {
        self.0.iter()
    }
}

// Conversions from one view into another view.

impl From<&[ir::ColumnEntry]> for CompilerCols {
    fn from(ir_cols: &[ir::ColumnEntry]) -> Self {
        CompilerCols(
            ir_cols
                .iter()
                // It's an one-to-one mapping from FLIR's JSON representation
                // to this intermediate representation.
                .map(|col| CompilerCol {
                    name: col.path.clone(),
                    ty: col.col_type.clone(),
                    // Foreign keys are encoded in the `ty` for a CompilerColumn.
                    // Hence, references becomes the unit type.
                    references: (),
                })
                .collect(),
        )
    }
}

impl From<&[CompilerCol]> for StoreEngineCols {
    fn from(compiler_cols: &[CompilerCol]) -> Self {
        let prepended_row_id_cols = StoreEngineCols::implicit_row_id_cols().into_iter();
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
        StoreEngineCols(prepended_row_id_cols.chain(schema_cols).collect())
    }
}

impl From<&[StoreEngineCol]> for QueryEngineCols {
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
