// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

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
