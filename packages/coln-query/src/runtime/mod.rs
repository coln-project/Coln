// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{
    api::deltas::{DerivedDataDelta, RowDelta, StoreDelta},
    error::RuntimeError,
};

pub trait Runtime<I, O>
where
    I: InputHandle,
    O: OutputHandle,
{
    fn apply(&mut self, delta: StoreDelta);
    fn run(&mut self) -> Result<DerivedDataDelta, RuntimeError>;
}

pub trait InputHandle {
    fn feed<I: IntoIterator<Item = RowDelta>>(&mut self, delta: I);
}

pub trait OutputHandle {
    fn fetch(&self) -> impl Iterator<Item = RowDelta>;
}
