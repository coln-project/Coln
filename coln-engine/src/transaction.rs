use tracing::info;

use crate::store::{Store, StoreIntError};

pub struct OwnedTransaction<F, O>
where
    F: FnOnce(&mut Store) -> Result<O, StoreIntError>,
{
    store: Store,
    f: F,
}

impl<F, O> OwnedTransaction<F, O>
where
    F: FnOnce(&mut Store) -> Result<O, StoreIntError>,
{
    pub fn new(store: Store, f: F) -> Self {
        OwnedTransaction { store, f }
    }

    pub fn commit(self) -> Result<(O, Store), (StoreIntError, Store)> {
        info!("start transaction");
        let mut preview_store = self.store.clone();
        let r = (self.f)(&mut preview_store).map_err(|err| (err, self.store.clone()))?;
        // TODO add primary key check
        preview_store
            .check_laws()
            .map_err(|err| (err, self.store.clone()))?;
        info!("transaction successful");
        Ok((r, preview_store))
    }
}

#[cfg(test)]
mod tests {
    use crate::ir::{ColType, Path, PrimType, Schema};
    use crate::store::test_support::link_foreign_key_theory;
    use crate::store::{Store, StoreIntError};
    use crate::table::{CellValue, Table, ValidationError};

    #[test]
    fn commit_returns_value_and_updated_store() {
        let path = Path::from("T");
        let schema = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        };
        let mut store = Store::new();
        store.insert_table(path.clone(), Table::new(path.clone(), schema));

        let tx = store.into_transaction(|s| {
            let oid = s.resolve_table(&path).expect("T");
            let t = s.table_mut(oid).expect("T");
            t.append_row_validated(vec![CellValue::Int(42)])?;
            Ok("done")
        });

        let (out, committed) = tx.commit().expect("commit");
        assert_eq!(out, "done");
        assert_eq!(committed.table_at(&path).expect("T").row_count(), 1);
        assert_eq!(
            committed.table_at(&path).expect("T").cell_at(0, 0),
            Some(&CellValue::Int(42))
        );
    }

    #[test]
    fn closure_err_returns_original_store() {
        let path = Path::from("T");
        let schema = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        };
        let mut store = Store::new();
        store.insert_table(path.clone(), Table::new(path.clone(), schema));

        let tx = store.into_transaction(|s| -> Result<(), StoreIntError> {
            let oid = s.resolve_table(&path).expect("T");
            let t = s.table_mut(oid).expect("T");
            t.append_row_validated(vec![CellValue::Int(1)])?;
            Err(StoreIntError::Validation(ValidationError::UnknownTable {
                path: Path::from("never"),
            }))
        });

        let (err, recovered) = tx.commit().unwrap_err();
        assert!(matches!(
            err,
            StoreIntError::Validation(ValidationError::UnknownTable { .. })
        ));
        assert_eq!(recovered.table_at(&path).expect("T").row_count(), 0);
    }

    #[test]
    fn law_err_returns_original_store() {
        let theory = link_foreign_key_theory();
        let link = Path::from("Link");
        let store = Store::try_from_theory(theory).expect("theory");

        let tx = store.into_transaction(|s| {
            let oid = s.resolve_table(&link).expect("Link");
            let t = s.table_mut(oid).expect("Link");
            t.append_row_validated(vec![CellValue::Int(10), CellValue::Int(20)])?;
            Ok(())
        });

        let (err, recovered) = tx.commit().unwrap_err();
        assert!(matches!(err, StoreIntError::Law(_)));
        assert_eq!(recovered.table_at(&link).expect("Link").row_count(), 0);
    }
}
