// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::host::variable::VariableSlot;

pub trait MemAddr {
    /// Make sure that the value is not moved in memory!
    fn mem_addr(&self) -> usize {
        self as *const Self as *const () as usize
    }
}

pub trait Resolvable {
    fn set_resolved(&mut self, resolved: VariableSlot);
}

pub trait Named {
    fn name(&self) -> &str;
}

/// An AST node identifier.
/// Can be its address in memory if using a pointer-based AST
/// or its index if using a flattened AST.
#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
pub struct NodeRef(usize);

impl From<usize> for NodeRef {
    fn from(index: usize) -> Self {
        Self(index)
    }
}

impl<T: MemAddr> From<&T> for NodeRef {
    fn from(addr: &T) -> Self {
        Self(addr.mem_addr())
    }
}

/// Generates the two ways of putting a node payload into its enum: from the
/// bare payload (which has to allocate) and from an already boxed one (which
/// reuses that allocation).
///
/// The boxed impl is what makes an owned rewriting pass cheap: the `…VisitorOwn`
/// families hand out `Box<XxxExpr>` precisely so that a node the pass leaves
/// alone can go back into its enum without a round trip through the allocator.
#[macro_export]
macro_rules! impl_from_auto_box {
    ($enum:ty, $(($variant:path, $expr:ty)),*) => {
            $(
                impl From<$expr> for $enum {
                    fn from(value: $expr) -> Self {
                        $variant(Box::new(value))
                    }
                }
                impl From<Box<$expr>> for $enum {
                    fn from(value: Box<$expr>) -> Self {
                        $variant(value)
                    }
                }
            )*
    }
}
