// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

pub(crate) trait Rollback {
    type Snapshot;

    fn snapshot(&mut self) -> Self::Snapshot;
    fn commit_snapshot(&mut self, snapshot: Self::Snapshot);
    fn rollback(&mut self, snapshot: Self::Snapshot);
}

// pub(crate) struct RollbackGuard<'a, R: Rollback> {
//     target: &'a mut R,
//     snap: R::Snapshot,
//     open: bool,
// }

// impl<'a, R: Rollback> RollbackGuard<'a, R> {
//     pub(crate) fn new(target: &'a mut R) -> Self {
//         Self {}
//     }
// }

// impl<'a, R: Rollback> Drop for RollbackGuard<'a, R> {
//     fn drop(&mut self) {
//         if !self.open {
//             self.target.rollback(self.snap);
//         }
//     }
// }

// pub(crate) trait AutoRollback: Rollback + Sized {
//     fn begin(&mut self) -> RollbackGuard<'_, Self> {
//         RollbackGuard::new(self)
//     }
// }

// impl<R: Rollback> AutoRollback for R {}
