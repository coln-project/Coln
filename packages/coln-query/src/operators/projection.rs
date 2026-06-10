// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{
    context::InterpreterContext,
    expr::Expr,
    interpreter::Interpreter,
    relation::{RelationSchema, Tuple, TupleKey, TupleValue},
    scalar::ScalarTypedValue,
};

pub fn projection_helper(attributes: &[(String, Expr)]) -> ProjectionStrategy<'_> {
    let requires_projection = attributes
        .iter()
        .any(|(_, expr)| is_pickable(expr).is_none());
    // We disable the pick optimization for now, as it may cause trouble with
    // column ordering.
    let requires_projection = true;

    if requires_projection {
        ProjectionStrategy::Projection(ProjectionHelper::new(attributes))
    } else {
        ProjectionStrategy::Pick(PickHelper::new(attributes))
    }
}

pub enum ProjectionStrategy<'a> {
    Projection(ProjectionHelper),
    Pick(PickHelper<'a>),
}

pub struct ProjectionHelper {
    attributes: Vec<String>,
    maps: Vec<Expr>,
}

impl ProjectionHelper {
    fn new(attributes: &[(String, Expr)]) -> Self {
        let (attributes, maps) = attributes.iter().cloned().unzip();
        Self { attributes, maps }
    }
    pub fn prepare(
        self,
        schema: &RelationSchema,
    ) -> (
        RelationSchema,
        impl Fn(InterpreterContext) -> (TupleKey, TupleValue) + use<> + Clone,
    ) {
        let schema = schema.project(self.attributes);
        let projection = move |mut ctx: InterpreterContext| {
            let value: TupleValue = self
                .maps
                .iter()
                .map(|map| {
                    ScalarTypedValue::try_from(
                        Interpreter::new(None)
                            .evaluate(map, &mut ctx)
                            .expect("Runtime error while interpreting projection function"),
                    )
                    .expect("Type error while interpreting projection function")
                })
                .collect();
            (TupleKey::empty(), value)
        };
        (schema, projection)
    }
}

pub struct PickHelper<'a> {
    /// First element is the target name of the attribute.
    /// Second element is the source name of the attribute,
    /// if different from the target name.
    attributes: Vec<(&'a String, Option<&'a String>)>,
}

impl<'a> PickHelper<'a> {
    fn new(attributes: &'a [(String, Expr)]) -> Self {
        let attributes = attributes
            .iter()
            .map(|(target_name, expr)| {
                is_pickable(expr)
                    .map(|source_name| (source_name, Some(target_name)))
                    .expect("Non-pick expression in pick helper")
            })
            .collect();
        Self { attributes }
    }
    pub fn prepare(&self, schema: &RelationSchema) -> RelationSchema {
        schema.pick(&self.attributes)
    }
}

/// If the passed expression is _exclusively_ referencing a **tuple** variable
/// and not containing an alias (that is a "."), `Some` is returned with the
/// variable's name. Otherwise, `None` is returned.
///
/// An alias requires the interpreter to be run to have variables be
/// named according to their alias.
pub fn is_pickable(expr: &Expr) -> Option<&String> {
    match expr {
        // Unresolved variables are variables from a tuple context.
        Expr::Var(inner) => {
            if inner.resolved.is_none() && !inner.name.contains(".") {
                Some(&inner.name)
            } else {
                None
            }
        }
        _ => None,
    }
}
