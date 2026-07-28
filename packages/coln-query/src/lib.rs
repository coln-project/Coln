#![allow(dead_code, unused_variables, rustdoc::private_intra_doc_links)]

// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

pub mod api;
pub mod error;
pub mod host;
pub mod optimizer;
pub mod pipeline;
pub mod relational;
pub mod scalarial;
pub mod test_helper;
mod typing;
mod util;

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        error::QueryEngineError,
        host::{
            expr::{
                AssignExpr, BinaryExpr, CallExpr, Expr, FunctionExpr, Literal, LiteralExpr, VarExpr,
            },
            operator::Operator,
            stmt::{BlockStmt, ExprStmt, Stmt, VarStmt},
            variable::Value,
        },
        pipeline::Pipeline,
        relational::{
            Runtime,
            expr::{
                AliasExpr, CartesianProductExpr, DifferenceExpr, DistinctExpr, EquiJoinExpr,
                FixedPointIterExpr, OutputExpr, OutputKind, ProjectionExpr, SelectionExpr, SinkId,
                SourceExpr, SourceId, UnionExpr,
            },
            incremental::dbsp::{ZWeight, zset},
            relation::TupleValue,
        },
        scalarial::ScalarTypedValue,
        test_helper::{person_profession_data, rows, rows_with_weight},
    };
    use ::dbsp::OrdZSet;
    use test_helper::{Edge, InputEntity, Person, PlainRelation, PredRel, Profession, SetOp};

    /// Tap the relation held by the variable `name` as a named runtime output,
    /// reusing the variable name as the output's [`SinkId`]. The resulting
    /// statement replaces the old "last value is output" idiom.
    fn output_stmt(name: &str) -> Stmt {
        Stmt::from(ExprStmt {
            expr: Expr::from(OutputExpr {
                relation: Expr::from(VarExpr::new(name)),
                id: SinkId::from(name),
                kind: OutputKind::Channel,
            }),
        })
    }

    // A function with two parameters which adds two values.
    fn add_func_expr() -> Expr {
        Expr::from(FunctionExpr {
            parameters: vec!["a".to_string(), "b".to_string()],
            body: BlockStmt {
                stmts: vec![Stmt::from(ExprStmt {
                    expr: Expr::from(BinaryExpr {
                        operator: Operator::Addition,
                        left: Expr::from(VarExpr::new("a")),
                        right: Expr::from(VarExpr::new("b")),
                    }),
                })],
            },
        })
    }

    #[test]
    fn test_variable_init_assign() -> Result<(), QueryEngineError> {
        // Initialization evaluates to the initialized value.
        let initialization = vec![Stmt::from(VarStmt {
            name: "a".to_string(),
            initializer: Some(Expr::from(LiteralExpr {
                value: Literal::Uint(1),
            })),
        })];
        assert_eq!(Pipeline::run(initialization)?.unwrap(), Value::Uint(1));

        // Reassignment evaluates to the assigned value.
        let reassignment = vec![
            Stmt::from(VarStmt {
                name: "a".to_string(),
                initializer: Some(Expr::from(LiteralExpr {
                    value: Literal::Uint(1),
                })),
            }),
            Stmt::from(ExprStmt {
                expr: Expr::from(AssignExpr::new(
                    "a",
                    Expr::from(LiteralExpr {
                        value: Literal::Uint(2),
                    }),
                )),
            }),
        ];
        assert_eq!(Pipeline::run(reassignment)?.unwrap(), Value::Uint(2));

        Ok(())
    }

    #[test]
    fn test_function_declarations() -> Result<(), QueryEngineError> {
        let anonymous_function = vec![Stmt::from(ExprStmt {
            expr: add_func_expr(),
        })];

        let named_function = vec![Stmt::from(VarStmt {
            name: "add".to_string(),
            initializer: Some(add_func_expr()),
        })];

        let result = Pipeline::run(anonymous_function)?.unwrap();
        assert_eq!(format!("{result}"), "<anonymous fn(a, b)>");

        let result = Pipeline::run(named_function)?.unwrap();
        assert_eq!(format!("{result}"), "<fn add(a, b)>");

        Ok(())
    }

    #[test]
    fn test_function_call() -> Result<(), QueryEngineError> {
        let function_call = vec![
            Stmt::from(VarStmt {
                name: "add".to_string(),
                initializer: Some(add_func_expr()),
            }),
            Stmt::from(ExprStmt {
                expr: Expr::from(CallExpr {
                    callee: Expr::from(VarExpr::new("add")),
                    arguments: vec![
                        Expr::from(LiteralExpr {
                            value: Literal::Uint(1),
                        }),
                        Expr::from(LiteralExpr {
                            value: Literal::Uint(2),
                        }),
                    ],
                }),
            }),
        ];

        let result = Pipeline::run(function_call)?.unwrap();
        assert_eq!(Value::Uint(3), result);

        Ok(())
    }

    #[test]
    fn test_selection_and_projection() -> Result<(), anyhow::Error> {
        let plan = vec![
            Stmt::from(VarStmt {
                name: "add".to_string(),
                initializer: Some(add_func_expr()),
            }),
            Stmt::from(VarStmt {
                name: "constant".to_string(),
                initializer: Some(Expr::from(LiteralExpr {
                    value: Literal::Uint(1),
                })),
            }),
            Stmt::from(VarStmt {
                name: "selected".to_string(),
                initializer: Some(Expr::from(SelectionExpr {
                    condition: Expr::from(BinaryExpr {
                        // Just to demonstrate logical operators.
                        // A `weight >= 2` is the outcome.
                        operator: Operator::Or,
                        left: Expr::from(BinaryExpr {
                            operator: Operator::Greater,
                            left: Expr::from(VarExpr::new("weight")),
                            // Just to demonstrate that we can call a function defined
                            // at the buildtime context from the runtime context.
                            right: Expr::from(CallExpr {
                                callee: Expr::from(VarExpr::new("add")),
                                arguments: vec![
                                    Expr::from(VarExpr::new("constant")),
                                    Expr::from(LiteralExpr {
                                        value: Literal::Uint(1),
                                    }),
                                ],
                            }),
                        }),
                        right: Expr::from(BinaryExpr {
                            operator: Operator::Equal,
                            left: Expr::from(VarExpr::new("weight")),
                            right: Expr::from(LiteralExpr::from(2_u64)),
                        }),
                    }),
                    relation: Expr::from(SourceExpr::new(Edge::schema())),
                })),
            }),
            output_stmt("selected"),
            Stmt::from(VarStmt {
                name: "projected".to_string(),
                initializer: Some(Expr::from(ProjectionExpr {
                    relation: Expr::from(VarExpr::new("selected")),
                    attributes: ["from", "to", "weight"]
                        .into_iter()
                        .map(|name| (name.to_string(), Expr::from(VarExpr::new(name))))
                        .chain([(
                            // Here we create an entirely new column.
                            "product_from_to".to_string(),
                            Expr::from(BinaryExpr {
                                operator: Operator::Multiplication,
                                left: Expr::from(VarExpr::new("from")),
                                right: Expr::from(VarExpr::new("to")),
                            }),
                        )])
                        .collect(),
                })),
            }),
            output_stmt("projected"),
        ];
        let mut rt = Pipeline::incremental().runtime(plan)?;

        const STEPS: usize = 3;

        let mut edges_data = ([
            [Edge::new(0, 1, 1), Edge::new(1, 2, 2), Edge::new(2, 3, 3)]
                .map(|e| (e, 2))
                .into_iter()
                .collect(),
            [Edge::new(3, 4, 1), Edge::new(4, 5, 2), Edge::new(5, 6, 3)]
                .map(|e| (e, 1))
                .into_iter()
                .collect(),
            [Edge::new(0, 1, 1), Edge::new(1, 2, 2), Edge::new(2, 3, 3)]
                .map(|e| (e, -1))
                .into_iter()
                .collect(),
        ] as [Vec<(Edge, ZWeight)>; STEPS])
            .into_iter();

        let mut selected_output = ([
            zset! {
                tuple!(1_u64, 2_u64, 2_u64, true) => 2,
                tuple!(2_u64, 3_u64, 3_u64, true) => 2,
            },
            zset! {
                tuple!(4_u64, 5_u64, 2_u64, true) => 1,
                tuple!(5_u64, 6_u64, 3_u64, true) => 1,
            },
            zset! {
                tuple!(1_u64, 2_u64, 2_u64, true) => -1,
                tuple!(2_u64, 3_u64, 3_u64, true) => -1,
            },
        ] as [OrdZSet<TupleValue>; STEPS])
            .into_iter();

        let mut projected_output = ([
            zset! {
                tuple!(1_u64, 2_u64, 2_u64, 2_u64) => 2,
                tuple!(2_u64, 3_u64, 3_u64, 6_u64) => 2,
            },
            zset! {
                tuple!(4_u64, 5_u64, 2_u64, 20_u64) => 1,
                tuple!(5_u64, 6_u64, 3_u64, 30_u64) => 1,
            },
            zset! {
                tuple!(1_u64, 2_u64, 2_u64, 2_u64) => -1,
                tuple!(2_u64, 3_u64, 3_u64, 6_u64) => -1,
            },
        ] as [OrdZSet<TupleValue>; STEPS])
            .into_iter();

        for _ in 1..=STEPS {
            rt.feed(&SourceId::from("edges"), rows(edges_data.next().unwrap()))?;
            rt.commit()?;
            assert_eq!(
                rt.output(&SinkId::from("selected"))?.0,
                selected_output.next().unwrap()
            );
            assert_eq!(
                rt.output(&SinkId::from("projected"))?.0,
                projected_output.next().unwrap()
            );
        }

        Ok(())
    }

    /// An [`OutputKind::Cli`] tap is a pass-through debug node: it prints the
    /// tapped relation to the CLI on every commit (run with `--nocapture` to see
    /// it) while leaving the relation untouched for downstream operators. Here a
    /// Cli tap sits on an intermediate `deduped`, which is then consumed further
    /// downstream; the downstream [`OutputKind::Channel`] output must still be
    /// correct, proving the tap is transparent.
    ///
    /// Note: printing a Cli tap drains its own read handle (the print calls
    /// `concat()`), so a Cli tap is print-only — use a [`OutputKind::Channel`]
    /// output to read rows back programmatically.
    #[test]
    fn test_cli_output_tap() -> Result<(), anyhow::Error> {
        let plan = vec![
            Stmt::from(VarStmt {
                name: "edges".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(Edge::schema()))),
            }),
            Stmt::from(VarStmt {
                name: "deduped".to_string(),
                initializer: Some(Expr::from(DistinctExpr {
                    relation: Expr::from(VarExpr::new("edges")),
                })),
            }),
            // Debug tap in the middle of the plan: prints on every commit.
            Stmt::from(ExprStmt {
                expr: Expr::from(OutputExpr {
                    relation: Expr::from(VarExpr::new("deduped")),
                    id: SinkId::from("deduped_trace"),
                    kind: OutputKind::Cli,
                }),
            }),
            // A downstream consumer of the tapped relation, exposed as a channel.
            Stmt::from(VarStmt {
                name: "downstream".to_string(),
                initializer: Some(Expr::from(DistinctExpr {
                    relation: Expr::from(VarExpr::new("deduped")),
                })),
            }),
            output_stmt("downstream"),
        ];
        let mut rt = Pipeline::incremental().runtime(plan)?;
        rt.feed(
            &SourceId::from("edges"),
            rows_with_weight([Edge::new(0, 1, 5)], 1),
        )?;
        rt.commit()?;
        // The Cli tap did not disturb the flow: the downstream channel is correct.
        // `Edge` carries an implicit `active` column (defaults to `true`).
        assert_eq!(
            rt.output(&SinkId::from("downstream"))?.0,
            zset! { tuple!(0_u64, 1_u64, 5_u64, true) => 1 }
        );
        // Reading the Cli tap by name fails loudly instead of returning drained,
        // empty data.
        let err = rt
            .output(&SinkId::from("deduped_trace"))
            .expect_err("reading a print-only Cli tap must error");
        assert!(
            err.to_string().contains("print-only"),
            "expected a print-only error, got: {err}"
        );
        Ok(())
    }

    /// Output names must be unique across the whole plan, regardless of kind.
    /// A `Cli` tap and a `Channel` output cannot share a name.
    #[test]
    fn test_duplicate_output_names_rejected() {
        let plan = vec![
            Stmt::from(VarStmt {
                name: "edges".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(Edge::schema()))),
            }),
            Stmt::from(VarStmt {
                name: "deduped".to_string(),
                initializer: Some(Expr::from(DistinctExpr {
                    relation: Expr::from(VarExpr::new("edges")),
                })),
            }),
            Stmt::from(ExprStmt {
                expr: Expr::from(OutputExpr {
                    relation: Expr::from(VarExpr::new("deduped")),
                    id: SinkId::from("dup"),
                    kind: OutputKind::Cli,
                }),
            }),
            Stmt::from(ExprStmt {
                expr: Expr::from(OutputExpr {
                    relation: Expr::from(VarExpr::new("deduped")),
                    id: SinkId::from("dup"),
                    kind: OutputKind::Channel,
                }),
            }),
        ];
        let Err(err) = Pipeline::incremental().runtime(plan) else {
            panic!("duplicate output names must be rejected at build time");
        };
        println!("{err}");
        assert!(
            err.to_string().contains("duplicate output name"),
            "expected a duplicate-name error, got: {err}"
        );
    }

    #[test]
    fn test_standard_join() -> Result<(), anyhow::Error> {
        let plan = vec![
            Stmt::from(VarStmt {
                name: "person".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(Person::schema()))),
            }),
            Stmt::from(VarStmt {
                name: "profession".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(Profession::schema()))),
            }),
            Stmt::from(VarStmt {
                name: "joined".to_string(),
                initializer: Some(Expr::from(EquiJoinExpr {
                    left: Expr::from(AliasExpr {
                        relation: Expr::from(VarExpr::new("person")),
                        alias: "pers".to_string(),
                    }),
                    right: Expr::from(AliasExpr {
                        relation: Expr::from(VarExpr::new("profession")),
                        alias: "prof".to_string(),
                    }),
                    // TODO: Shall we force aliasing here? Technically,
                    // it isn't required because the left attribute only
                    // operates on the left relation and the right
                    // attribute only operates on the right relation.
                    on: vec![(
                        Expr::from(VarExpr::new("profession_id")),
                        Expr::from(VarExpr::new("profession_id")),
                    )],
                    // attributes: None,
                    attributes: Some(
                        // Here, we filter out the duplicated profession_id
                        // column that occurs after the join.
                        [
                            ("person_id", "pers.person_id"),
                            ("person_name", "pers.name"),
                            ("age", "pers.age"),
                            ("profession_id", "prof.profession_id"),
                            ("profession_name", "prof.name"),
                        ]
                        .into_iter()
                        .map(|(name, identifier)| {
                            (name.to_string(), Expr::from(VarExpr::new(identifier)))
                        })
                        .collect(),
                    ),
                })),
            }),
            output_stmt("joined"),
        ];
        let mut rt = Pipeline::incremental().runtime(plan)?;

        for (person_step, profession_step) in person_profession_data() {
            rt.feed(&SourceId::from("person"), rows_with_weight(person_step, 1))?;
            rt.feed(
                &SourceId::from("profession"),
                rows_with_weight(profession_step, 1),
            )?;

            rt.commit()?;

            assert_eq!(
                rt.output(&SinkId::from("joined"))?.0,
                zset! {
                    tuple!(0_u64, "Alice", 20_u64, 0_u64, "Engineer") => 1,
                    tuple!(2_u64, "Charlie", 40_u64, 0_u64, "Engineer") => 1,
                    tuple!(1_u64, "Bob", 30_u64, 1_u64, "Doctor") => 1,
                }
            );
        }

        Ok(())
    }

    #[test]
    fn test_cartesian_product() -> Result<(), anyhow::Error> {
        let plan = vec![
            Stmt::from(VarStmt {
                name: "person".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(Person::schema()))),
            }),
            Stmt::from(VarStmt {
                name: "profession".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(Profession::schema()))),
            }),
            Stmt::from(VarStmt {
                name: "joined".to_string(),
                initializer: Some(Expr::from(CartesianProductExpr::new(
                    Expr::from(AliasExpr {
                        relation: Expr::from(VarExpr::new("person")),
                        alias: "pers".to_string(),
                    }),
                    Expr::from(AliasExpr {
                        relation: Expr::from(VarExpr::new("profession")),
                        alias: "prof".to_string(),
                    }),
                    None,
                ))),
            }),
            output_stmt("joined"),
        ];
        let mut rt = Pipeline::incremental().runtime(plan)?;

        for (person_step, profession_step) in person_profession_data() {
            rt.feed(&SourceId::from("person"), rows_with_weight(person_step, 1))?;
            rt.feed(
                &SourceId::from("profession"),
                rows_with_weight(profession_step, 1),
            )?;

            rt.commit()?;

            assert_eq!(
                rt.output(&SinkId::from("joined"))?.0,
                zset! {
                    tuple!(0_u64, "Alice", 20_u64, 0_u64, 0_u64, "Engineer") => 1,
                    tuple!(0_u64, "Alice", 20_u64, 0_u64, 1_u64, "Doctor") => 1,
                    tuple!(1_u64, "Bob", 30_u64, 1_u64, 0_u64, "Engineer") => 1,
                    tuple!(1_u64, "Bob", 30_u64, 1_u64, 1_u64, "Doctor") => 1,
                    tuple!(2_u64, "Charlie", 40_u64, 0_u64, 0_u64, "Engineer") => 1,
                    tuple!(2_u64, "Charlie", 40_u64, 0_u64, 1_u64, "Doctor") => 1,
                }
            );
        }

        Ok(())
    }

    #[test]
    fn test_self_join() -> Result<(), anyhow::Error> {
        let plan = vec![
            Stmt::from(VarStmt {
                name: "edges".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(Edge::schema()))),
            }),
            Stmt::from(VarStmt {
                name: "len_1".to_string(),
                initializer: Some(Expr::from(ProjectionExpr {
                    relation: Expr::from(VarExpr::new("edges")),
                    attributes: ["from", "to"]
                        .into_iter()
                        .map(|name| (name.to_string(), Expr::from(VarExpr::new(name))))
                        .chain(
                            [
                                ("cumulated_weight", Expr::from(VarExpr::new("weight"))),
                                (
                                    "hopcount",
                                    Expr::from(LiteralExpr {
                                        value: Literal::Uint(1),
                                    }),
                                ),
                            ]
                            .map(|(name, expr)| (name.to_string(), expr)),
                        )
                        .collect(),
                })),
            }),
            Stmt::from(VarStmt {
                name: "len_2".to_string(),
                initializer: Some(Expr::from(EquiJoinExpr {
                    left: Expr::from(AliasExpr {
                        relation: Expr::from(VarExpr::new("len_1")),
                        alias: "cur".to_string(),
                    }),
                    right: Expr::from(AliasExpr {
                        relation: Expr::from(VarExpr::new("edges")),
                        alias: "next".to_string(),
                    }),
                    on: vec![(
                        Expr::from(VarExpr::new("to")),
                        Expr::from(VarExpr::new("from")),
                    )],
                    attributes: Some(
                        [
                            ("start", Expr::from(VarExpr::new("cur.from"))),
                            ("end", Expr::from(VarExpr::new("next.to"))),
                            (
                                "cumulated_weight",
                                Expr::from(BinaryExpr {
                                    operator: Operator::Addition,
                                    left: Expr::from(VarExpr::new("cur.cumulated_weight")),
                                    right: Expr::from(VarExpr::new("next.weight")),
                                }),
                            ),
                            (
                                "hopcount",
                                Expr::from(BinaryExpr {
                                    operator: Operator::Addition,
                                    left: Expr::from(VarExpr::new("cur.hopcount")),
                                    right: Expr::from(LiteralExpr {
                                        value: Literal::Uint(1),
                                    }),
                                }),
                            ),
                        ]
                        .into_iter()
                        .map(|(name, expr)| (name.to_string(), expr))
                        .collect(),
                    ),
                })),
            }),
            Stmt::from(VarStmt {
                name: "len_3".to_string(),
                initializer: Some(Expr::from(EquiJoinExpr {
                    left: Expr::from(AliasExpr {
                        relation: Expr::from(VarExpr::new("len_2")),
                        alias: "cur".to_string(),
                    }),
                    right: Expr::from(AliasExpr {
                        relation: Expr::from(VarExpr::new("edges")),
                        alias: "next".to_string(),
                    }),
                    on: vec![(
                        Expr::from(VarExpr::new("end")),
                        Expr::from(VarExpr::new("from")),
                    )],
                    attributes: Some(
                        [
                            ("start", Expr::from(VarExpr::new("cur.start"))),
                            ("end", Expr::from(VarExpr::new("next.to"))),
                            (
                                "cumulated_weight",
                                Expr::from(BinaryExpr {
                                    operator: Operator::Addition,
                                    left: Expr::from(VarExpr::new("cur.cumulated_weight")),
                                    right: Expr::from(VarExpr::new("next.weight")),
                                }),
                            ),
                            (
                                "hopcount",
                                Expr::from(BinaryExpr {
                                    operator: Operator::Addition,
                                    left: Expr::from(VarExpr::new("cur.hopcount")),
                                    right: Expr::from(LiteralExpr {
                                        value: Literal::Uint(1),
                                    }),
                                }),
                            ),
                        ]
                        .into_iter()
                        .map(|(name, expr)| (name.to_string(), expr))
                        .collect(),
                    ),
                })),
            }),
            Stmt::from(VarStmt {
                name: "len_4".to_string(),
                initializer: Some(Expr::from(EquiJoinExpr {
                    left: Expr::from(AliasExpr {
                        relation: Expr::from(VarExpr::new("len_3")),
                        alias: "cur".to_string(),
                    }),
                    right: Expr::from(AliasExpr {
                        relation: Expr::from(VarExpr::new("edges")),
                        alias: "next".to_string(),
                    }),
                    on: vec![(
                        Expr::from(VarExpr::new("end")),
                        Expr::from(VarExpr::new("from")),
                    )],
                    attributes: Some(
                        [
                            ("start", Expr::from(VarExpr::new("cur.start"))),
                            ("end", Expr::from(VarExpr::new("next.to"))),
                            (
                                "cumulated_weight",
                                Expr::from(BinaryExpr {
                                    operator: Operator::Addition,
                                    left: Expr::from(VarExpr::new("cur.cumulated_weight")),
                                    right: Expr::from(VarExpr::new("next.weight")),
                                }),
                            ),
                            (
                                "hopcount",
                                Expr::from(BinaryExpr {
                                    operator: Operator::Addition,
                                    left: Expr::from(VarExpr::new("cur.hopcount")),
                                    right: Expr::from(LiteralExpr {
                                        value: Literal::Uint(1),
                                    }),
                                }),
                            ),
                        ]
                        .into_iter()
                        .map(|(name, expr)| (name.to_string(), expr))
                        .collect(),
                    ),
                })),
            }),
            Stmt::from(VarStmt {
                name: "full_closure".to_string(),
                initializer: Some(Expr::from(UnionExpr {
                    relations: ["len_1", "len_2", "len_3", "len_4"]
                        .into_iter()
                        .map(|name| Expr::from(VarExpr::new(name)))
                        .collect(),
                })),
            }),
            output_stmt("full_closure"),
        ];
        let mut rt = Pipeline::incremental().runtime(plan)?;

        let init_data = [
            Edge::new(0, 1, 1),
            // This edge is omitted: Edge::new(1, 2, 1),
            Edge::new(2, 3, 2),
            Edge::new(3, 4, 2),
        ];

        rt.feed(&SourceId::from("edges"), rows_with_weight(init_data, 1))?;

        rt.commit()?;

        assert_eq!(
            rt.output(&SinkId::from("full_closure"))?.0,
            zset! {
                tuple!(0_u64, 1_u64, 1_u64, 1_u64) => 1,
                tuple!(2_u64, 3_u64, 2_u64, 1_u64) => 1,
                tuple!(2_u64, 4_u64, 4_u64, 2_u64) => 1,
                tuple!(3_u64, 4_u64, 2_u64, 1_u64) => 1,
            }
        );

        let extra_data = [Edge::new(1, 2, 1)];

        rt.feed(&SourceId::from("edges"), rows_with_weight(extra_data, 1))?;

        rt.commit()?;

        assert_eq!(
            rt.output(&SinkId::from("full_closure"))?.0,
            zset! {
                tuple!(0_u64, 2_u64, 2_u64, 2_u64) => 1,
                tuple!(1_u64, 2_u64, 1_u64, 1_u64) => 1,
                tuple!(0_u64, 3_u64, 4_u64, 3_u64) => 1,
                tuple!(1_u64, 3_u64, 3_u64, 2_u64) => 1,
                tuple!(0_u64, 4_u64, 6_u64, 4_u64) => 1,
                tuple!(1_u64, 4_u64, 5_u64, 3_u64) => 1,
            }
        );

        Ok(())
    }

    #[test]
    fn test_iteration() -> Result<(), anyhow::Error> {
        let plan = vec![
            Stmt::from(VarStmt {
                name: "edges".to_string(),
                initializer: Some(Expr::from(ProjectionExpr {
                    relation: Expr::from(SourceExpr::new(Edge::schema())),
                    attributes: ["from", "to", "weight"]
                        .into_iter()
                        .map(|name| (name.to_string(), Expr::from(VarExpr::new(name))))
                        .collect(),
                })),
            }),
            Stmt::from(VarStmt {
                name: "base".to_string(),
                initializer: Some(Expr::from(ProjectionExpr {
                    relation: Expr::from(VarExpr::new("edges")),
                    attributes: ["from", "to"]
                        .into_iter()
                        .map(|name| (name.to_string(), Expr::from(VarExpr::new(name))))
                        .chain(
                            [
                                ("cumulated_weight", Expr::from(VarExpr::new("weight"))),
                                (
                                    "hopcount",
                                    Expr::from(LiteralExpr {
                                        value: Literal::Uint(1),
                                    }),
                                ),
                            ]
                            .map(|(name, expr)| (name.to_string(), expr)),
                        )
                        .collect(),
                })),
            }),
            Stmt::from(VarStmt {
                name: "closure".to_string(),
                initializer: Some(Expr::from(FixedPointIterExpr {
                    // `edges` is referenced in the step below and is auto-bridged
                    // into the iteration — no explicit imports needed.
                    accumulator: ("accumulator".to_string(), Expr::from(VarExpr::new("base"))),
                    step: BlockStmt {
                        stmts: vec![Stmt::from(ExprStmt {
                            expr: Expr::from(EquiJoinExpr {
                                left: Expr::from(AliasExpr {
                                    relation: Expr::from(VarExpr::new("accumulator")),
                                    alias: "cur".to_string(),
                                }),
                                right: Expr::from(AliasExpr {
                                    relation: Expr::from(VarExpr::new("edges")),
                                    alias: "next".to_string(),
                                }),
                                on: vec![(
                                    Expr::from(VarExpr::new("to")),
                                    Expr::from(VarExpr::new("from")),
                                )],
                                attributes: Some(
                                    [
                                        ("start", Expr::from(VarExpr::new("cur.from"))),
                                        ("end", Expr::from(VarExpr::new("next.to"))),
                                        (
                                            "cumulated_weight",
                                            Expr::from(BinaryExpr {
                                                operator: Operator::Addition,
                                                left: Expr::from(VarExpr::new(
                                                    "cur.cumulated_weight",
                                                )),
                                                right: Expr::from(VarExpr::new("next.weight")),
                                            }),
                                        ),
                                        (
                                            "hopcount",
                                            Expr::from(BinaryExpr {
                                                operator: Operator::Addition,
                                                left: Expr::from(VarExpr::new("cur.hopcount")),
                                                right: Expr::from(LiteralExpr {
                                                    value: Literal::Uint(1),
                                                }),
                                            }),
                                        ),
                                    ]
                                    .into_iter()
                                    .map(|(name, expr)| (name.to_string(), expr))
                                    .collect(),
                                ),
                            }),
                        })],
                    },
                })),
            }),
            output_stmt("closure"),
        ];
        let mut rt = Pipeline::incremental().runtime(plan)?;

        let init_data = [
            Edge::new(0, 1, 1),
            Edge::new(1, 2, 1),
            Edge::new(2, 3, 2),
            Edge::new(3, 4, 2),
        ];

        rt.feed(&SourceId::from("edges"), rows_with_weight(init_data, 1))?;

        rt.commit()?;

        assert_eq!(
            rt.output(&SinkId::from("closure"))?.0,
            zset! {
                tuple!(0_u64, 1_u64, 1_u64, 1_u64) => 1,
                tuple!(0_u64, 2_u64, 2_u64, 2_u64) => 1,
                tuple!(1_u64, 2_u64, 1_u64, 1_u64) => 1,
                tuple!(0_u64, 3_u64, 4_u64, 3_u64) => 1,
                tuple!(1_u64, 3_u64, 3_u64, 2_u64) => 1,
                tuple!(2_u64, 3_u64, 2_u64, 1_u64) => 1,
                tuple!(0_u64, 4_u64, 6_u64, 4_u64) => 1,
                tuple!(1_u64, 4_u64, 5_u64, 3_u64) => 1,
                tuple!(2_u64, 4_u64, 4_u64, 2_u64) => 1,
                tuple!(3_u64, 4_u64, 2_u64, 1_u64) => 1,
            }
        );

        Ok(())
    }

    #[test]
    fn source_leaf_inside_fixed_point_step_is_bridged() -> Result<(), anyhow::Error> {
        // A `SourceExpr` referenced *only* inside a step body is legal: the
        // backend wires its root input and `delta0`s it into the nested circuit,
        // exactly as it would an outer variable — no explicit imports needed.
        // This computes reachability: starting from the seed nodes in `plain`,
        // follow `edges` transitively. `edges` appears nowhere but the step, so
        // this exercises wiring a source's root input for a step-only source.
        let plan = vec![
            // base = the seed node ids from `plain`, as a single `node` column.
            Stmt::from(VarStmt {
                name: "base".to_string(),
                initializer: Some(Expr::from(ProjectionExpr {
                    relation: Expr::from(SourceExpr::new(PlainRelation::schema())),
                    attributes: vec![("node".to_string(), Expr::from(VarExpr::new("a")))],
                })),
            }),
            Stmt::from(VarStmt {
                name: "reachable".to_string(),
                initializer: Some(Expr::from(FixedPointIterExpr {
                    accumulator: ("reachable".to_string(), Expr::from(VarExpr::new("base"))),
                    step: BlockStmt {
                        stmts: vec![Stmt::from(ExprStmt {
                            expr: Expr::from(EquiJoinExpr {
                                left: Expr::from(AliasExpr {
                                    relation: Expr::from(VarExpr::new("reachable")),
                                    alias: "cur".to_string(),
                                }),
                                // `edges` used inline in the step — its only use.
                                right: Expr::from(AliasExpr {
                                    relation: Expr::from(SourceExpr::new(Edge::schema())),
                                    alias: "edge".to_string(),
                                }),
                                on: vec![(
                                    Expr::from(VarExpr::new("node")),
                                    Expr::from(VarExpr::new("from")),
                                )],
                                attributes: Some(vec![(
                                    "node".to_string(),
                                    Expr::from(VarExpr::new("edge.to")),
                                )]),
                            }),
                        })],
                    },
                })),
            }),
            output_stmt("reachable"),
        ];

        let mut rt = Pipeline::incremental().runtime(plan)?;

        // Seed node 0; edges 0->1->2->3.
        rt.feed(
            &SourceId::from("plain"),
            rows([(PlainRelation::new(0, 0, 0), 1)]),
        )?;
        rt.feed(
            &SourceId::from("edges"),
            rows_with_weight(
                [Edge::new(0, 1, 1), Edge::new(1, 2, 1), Edge::new(2, 3, 1)],
                1,
            ),
        )?;
        rt.commit()?;

        assert_eq!(
            rt.output(&SinkId::from("reachable"))?.0,
            zset! {
                tuple!(0_u64) => 1,
                tuple!(1_u64) => 1,
                tuple!(2_u64) => 1,
                tuple!(3_u64) => 1,
            }
        );

        Ok(())
    }

    #[test]
    fn test_mvr_store_crdt() -> Result<(), anyhow::Error> {
        let plan = vec![
            // Inputs start.
            Stmt::from(VarStmt {
                name: "pred".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(PredRel::schema()))),
            }),
            Stmt::from(VarStmt {
                name: "set".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(SetOp::schema()))),
            }),
            // Inputs end.
            Stmt::from(VarStmt {
                name: "overwritten".to_string(),
                initializer: Some(Expr::from(DistinctExpr {
                    relation: Expr::from(ProjectionExpr {
                        relation: Expr::from(VarExpr::new("pred")),
                        attributes: [("RepId", "FromRepId"), ("Ctr", "FromCtr")]
                            .into_iter()
                            .map(|(name, origin)| {
                                (name.to_string(), Expr::from(VarExpr::new(origin)))
                            })
                            .collect(),
                    }),
                })),
            }),
            Stmt::from(VarStmt {
                name: "overwrites".to_string(),
                initializer: Some(Expr::from(DistinctExpr {
                    relation: Expr::from(ProjectionExpr {
                        relation: Expr::from(VarExpr::new("pred")),
                        attributes: [("RepId", "ToRepId"), ("Ctr", "ToCtr")]
                            .into_iter()
                            .map(|(name, origin)| {
                                (name.to_string(), Expr::from(VarExpr::new(origin)))
                            })
                            .collect(),
                    }),
                })),
            }),
            Stmt::from(VarStmt {
                name: "isRoot".to_string(),
                initializer: Some(Expr::from(DifferenceExpr {
                    left: Expr::from(ProjectionExpr {
                        relation: Expr::from(VarExpr::new("set")),
                        attributes: ["RepId", "Ctr"]
                            .into_iter()
                            .map(|name| (name.to_string(), Expr::from(VarExpr::new(name))))
                            .collect(),
                    }),
                    right: Expr::from(VarExpr::new("overwrites")),
                })),
            }),
            Stmt::from(VarStmt {
                name: "isLeaf".to_string(),
                initializer: Some(Expr::from(DifferenceExpr {
                    left: Expr::from(ProjectionExpr {
                        relation: Expr::from(VarExpr::new("set")),
                        attributes: ["RepId", "Ctr"]
                            .into_iter()
                            .map(|name| (name.to_string(), Expr::from(VarExpr::new(name))))
                            .collect(),
                    }),
                    right: Expr::from(VarExpr::new("overwritten")),
                })),
            }),
            Stmt::from(VarStmt {
                name: "isCausallyReady".to_string(),
                initializer: Some(Expr::from(FixedPointIterExpr {
                    // `pred` is referenced in the step below and is auto-bridged
                    // into the iteration — no explicit imports needed.
                    accumulator: (
                        "isCausallyReady".to_string(),
                        Expr::from(VarExpr::new("isRoot")),
                    ),
                    step: BlockStmt {
                        stmts: vec![Stmt::from(ExprStmt {
                            expr: Expr::from(EquiJoinExpr {
                                left: Expr::from(AliasExpr {
                                    relation: Expr::from(VarExpr::new("isCausallyReady")),
                                    alias: "cur".to_string(),
                                }),
                                right: Expr::from(AliasExpr {
                                    relation: Expr::from(VarExpr::new("pred")),
                                    alias: "next".to_string(),
                                }),
                                on: vec![
                                    (
                                        Expr::from(VarExpr::new("RepId")),
                                        Expr::from(VarExpr::new("FromRepId")),
                                    ),
                                    (
                                        Expr::from(VarExpr::new("Ctr")),
                                        Expr::from(VarExpr::new("FromCtr")),
                                    ),
                                ],
                                attributes: Some(
                                    [
                                        ("RepId", Expr::from(VarExpr::new("next.ToRepId"))),
                                        ("Ctr", Expr::from(VarExpr::new("next.ToCtr"))),
                                    ]
                                    .into_iter()
                                    .map(|(name, expr)| (name.to_string(), expr))
                                    .collect(),
                                ),
                            }),
                        })],
                    },
                })),
            }),
            Stmt::from(VarStmt {
                name: "mvrStore".to_string(),
                initializer: Some(Expr::from(EquiJoinExpr {
                    left: Expr::from(VarExpr::new("isCausallyReady")),
                    right: Expr::from(EquiJoinExpr {
                        left: Expr::from(VarExpr::new("isLeaf")),
                        right: Expr::from(VarExpr::new("set")),
                        on: vec![
                            (
                                Expr::from(VarExpr::new("RepId")),
                                Expr::from(VarExpr::new("RepId")),
                            ),
                            (
                                Expr::from(VarExpr::new("Ctr")),
                                Expr::from(VarExpr::new("Ctr")),
                            ),
                        ],
                        // With `attributes: None` the query does not work because
                        // the fields `rep_id` and `ctr` are both duplicated in
                        // the tuple output. The EquiJoin below then indexes upon
                        // both duplicated fields for its `right` operand
                        // and no join match is found with its `left` operand.
                        // Welcome to the funny world of relational algebra's semantics
                        // under name collisions.
                        attributes: Some(
                            [
                                ("RepId", Expr::from(VarExpr::new("RepId"))),
                                ("Ctr", Expr::from(VarExpr::new("Ctr"))),
                                ("Key", Expr::from(VarExpr::new("Key"))),
                                ("Value", Expr::from(VarExpr::new("Value"))),
                            ]
                            .into_iter()
                            .map(|(name, expr)| (name.to_string(), expr))
                            .collect(),
                        ),
                    }),
                    on: vec![
                        (
                            Expr::from(VarExpr::new("RepId")),
                            Expr::from(VarExpr::new("RepId")),
                        ),
                        (
                            Expr::from(VarExpr::new("Ctr")),
                            Expr::from(VarExpr::new("Ctr")),
                        ),
                    ],
                    attributes: Some(
                        [
                            ("Key", Expr::from(VarExpr::new("Key"))),
                            ("Value", Expr::from(VarExpr::new("Value"))),
                        ]
                        .into_iter()
                        .map(|(name, expr)| (name.to_string(), expr))
                        .collect(),
                    ),
                })),
            }),
            output_stmt("mvrStore"),
        ];
        let mut rt = Pipeline::incremental().runtime(plan)?;

        // The operation history is as follows:
        // In first step (just one root operation setting register with key 1 to
        // value 1):
        //
        // set_0_0(1, 1)
        //
        // In second step (concurrent writes by replica 0 and 1):
        //
        //               ---> set_0_1(1, 2)
        // set_0_0(1, 1)
        //               ---> set_1_0(1, 3)
        //
        // In third step (replica 1 does a "merge" operation overwriting the
        // previous conflict):
        //
        //               ---> set_0_1(1, 2)
        // set_0_0(1, 1)                    ---> set_1_2(1, 4)
        //               ---> set_1_0(1, 3)
        //

        let pred_rel_data = [
            vec![],
            vec![PredRel::new(0, 0, 0, 1), PredRel::new(0, 0, 1, 0)],
            vec![PredRel::new(0, 1, 1, 2), PredRel::new(1, 0, 1, 2)],
        ];

        let set_op_data = [
            vec![SetOp::new(0, 0, 1, 1)],
            vec![SetOp::new(0, 1, 1, 2), SetOp::new(1, 0, 1, 3)],
            vec![SetOp::new(1, 2, 1, 4)],
        ];

        let mut expected = [
            zset! {
                tuple!(1_u64, 1_u64) => 1,
            },
            zset! {
                tuple!(1_u64, 1_u64) => -1,
                tuple!(1_u64, 2_u64) => 1,
                tuple!(1_u64, 3_u64) => 1,
            },
            zset! {
                tuple!(1_u64, 2_u64) => -1,
                tuple!(1_u64, 3_u64) => -1,
                tuple!(1_u64, 4_u64) => 1,
            },
        ]
        .into_iter();

        for (pred_rel_step, set_op_step) in pred_rel_data.into_iter().zip(set_op_data) {
            rt.feed(&SourceId::from("pred"), rows_with_weight(pred_rel_step, 1))?;
            rt.feed(&SourceId::from("set"), rows_with_weight(set_op_step, 1))?;

            rt.commit()?;

            assert_eq!(
                rt.output(&SinkId::from("mvrStore"))?.0,
                expected.next().unwrap()
            );
        }

        Ok(())
    }
}
