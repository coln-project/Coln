// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

pub(crate) trait Rollback {
    type Snapshot;

    fn snapshot(&mut self) -> Self::Snapshot;
    fn commit(&mut self, snapshot: Self::Snapshot);
    fn rollback_to(&mut self, snapshot: Self::Snapshot);
}
