// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::super::super::relation::RelationRef;
use super::super::dbsp::AsDbspRelation;
use super::super::schema::{DbspTupleContext, SchemaTuple, TupleKey};
use super::StreamWrapper;
use super::projection::is_pickable;
use crate::{
    host::{InterpreterContext, expr::Expr, variable::Environment},
    scalarial::{RowScalarEngine, ScalarTypedValue},
};
use std::rc::Rc;

pub fn reindex_helper<E: RowScalarEngine>(
    relation: &RelationRef,
    on: &[&Expr],
    environment: &Environment,
    engine: E,
) -> (StreamWrapper, Vec<String>) {
    let requires_projection = on.iter().any(|expr| is_pickable(expr).is_none());
    // We disable the pick optimization for now, as it may cause trouble with
    // column ordering.
    let requires_projection = true;

    let relation_ref = relation.borrow();

    if requires_projection {
        let schema: Vec<String> = on
            .iter()
            .enumerate()
            .map(|(idx, _)| format!("anonym_field_{idx}"))
            .collect();
        let indexed = relation_ref.as_dbsp().stream().map_index({
            let relation = Rc::clone(relation);
            // Compile each key expression once, off the per-tuple hot path.
            let programs = on
                .iter()
                .map(|map| engine.compile(map))
                .collect::<Result<Vec<_>, _>>()
                // TODO: beautify.
                .expect("Key expression compilation error");
            let environment = environment.clone();
            move |(_key, tuple)| {
                let relation_ref = relation.borrow();
                let schema = relation_ref.as_dbsp().schema();
                let environment = &mut environment.clone();
                let mut new_ctx = InterpreterContext::new(environment);
                new_ctx.extend_tuple_ctx(&None, &schema.tuple, tuple);
                let key: TupleKey = programs
                    .iter()
                    .map(|program| {
                        ScalarTypedValue::try_from(
                            engine
                                .run(program, &mut new_ctx)
                                .expect("Runtime error while interpreting projection function"),
                        )
                        .expect("Type error while interpreting projection function")
                    })
                    .collect();
                (key, tuple.clone())
            }
        });
        (indexed, schema)
    } else {
        let key_field_picks: Vec<String> = on
            .iter()
            .map(|expr| {
                is_pickable(expr)
                    .expect("Expected pickable expression")
                    .clone()
            })
            .collect();
        let indexed = relation_ref.as_dbsp().stream().map_index({
            let key_field_picks = key_field_picks.clone();
            let relation = Rc::clone(relation);
            move |(_key, tuple)| {
                let relation_ref = relation.borrow();
                let key: TupleKey = SchemaTuple::new(&relation_ref.as_dbsp().schema().tuple, tuple)
                    .pick(key_field_picks.as_slice())
                    .collect();
                (key, tuple.clone())
            }
        });
        (indexed, key_field_picks)
    }
}
