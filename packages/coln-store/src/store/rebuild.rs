// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::store::Store;
use crate::store::error::StoreError;
use crate::table::TableOid;

impl Store {
    /// Rebuild until a pass displaces no further ids, so a commit that merged
    /// nothing does no rebuild work at all.
    pub(super) fn rebuild_to_fixpoint(&mut self) -> Result<(), StoreError> {
        while self.rowing.has_displaced() {
            self.rebuild_one()?;
            tracing::debug!("finished one iteration of rebuilding");
        }
        Ok(())
    }

    fn rebuild_one(&mut self) -> Result<(), StoreError> {
        let affected = self.rebuild_tables();
        self.apply_staged_ops(&affected)
    }

    fn rebuild_tables(&mut self) -> Vec<TableOid> {
        for tbl in self.tables.values_mut() {
            tbl.rebuild(&self.rowing, &self.id_packer);
        }

        // clear up the displaced table because the changes have all been staged.
        self.rowing.clear_displaced();
        self.tables.keys().copied().collect()
    }
}
