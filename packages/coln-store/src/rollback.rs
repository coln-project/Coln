// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

pub(crate) trait Rollback {
    type Snapshot;

    fn snapshot(&mut self) -> Self::Snapshot;
    fn commit_snapshot(&mut self, snapshot: Self::Snapshot);
    fn rollback(&mut self, snapshot: Self::Snapshot);
}
