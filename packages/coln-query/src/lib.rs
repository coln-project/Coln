#![allow(dead_code, unused_variables, rustdoc::private_intra_doc_links)]

// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

pub mod api;
mod error;
mod host;
mod optimizer;
mod pipeline;
mod program;
mod relational;
mod scalarial;
#[cfg(feature = "test-utils")]
mod test_utils;
mod typing;
mod utils;

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
                SourceExpr, UnionExpr,
            },
            incremental::dbsp::{ZWeight, zset},
            relation::TupleValue,
        },
        scalarial::ScalarTypedValue,
        test_utils::{TestProgram, person_profession_data, rows, rows_with_weight},
    };
    use ::dbsp::OrdZSet;
    use test_utils::{EdgeRel, InputRel, PersonRel, PlainRel, PredRel, ProfessionRel, SetRel};

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
                    relation: Expr::from(SourceExpr::new(EdgeRel::id())),
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
        let mut rt =
            Pipeline::incremental().runtime(&mut TestProgram::new(plan, [EdgeRel::schema()]))?;

        const STEPS: usize = 3;

        let mut edges_data = ([
            [
                EdgeRel::new(0, 1, 1),
                EdgeRel::new(1, 2, 2),
                EdgeRel::new(2, 3, 3),
            ]
            .map(|e| (e, 2))
            .into_iter()
            .collect(),
            [
                EdgeRel::new(3, 4, 1),
                EdgeRel::new(4, 5, 2),
                EdgeRel::new(5, 6, 3),
            ]
            .map(|e| (e, 1))
            .into_iter()
            .collect(),
            [
                EdgeRel::new(0, 1, 1),
                EdgeRel::new(1, 2, 2),
                EdgeRel::new(2, 3, 3),
            ]
            .map(|e| (e, -1))
            .into_iter()
            .collect(),
        ] as [Vec<(EdgeRel, ZWeight)>; STEPS])
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
            assert!(rt.feed(&EdgeRel::id(), rows(edges_data.next().unwrap()))?);
            rt.commit()?;
            assert_eq!(
                rt.output(&SinkId::from("selected"))?
                    .debug_view(false)
                    .to_zset(),
                selected_output.next().unwrap()
            );
            assert_eq!(
                rt.output(&SinkId::from("projected"))?
                    .debug_view(false)
                    .to_zset(),
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
                initializer: Some(Expr::from(SourceExpr::new(EdgeRel::id()))),
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
        let mut rt =
            Pipeline::incremental().runtime(&mut TestProgram::new(plan, [EdgeRel::schema()]))?;
        assert!(rt.feed(&EdgeRel::id(), rows_with_weight([EdgeRel::new(0, 1, 5)], 1),)?);
        rt.commit()?;
        // The Cli tap did not disturb the flow: the downstream channel is correct.
        // `Edge` carries an implicit `active` column (defaults to `true`).
        assert_eq!(
            rt.output(&SinkId::from("downstream"))?
                .debug_view(false)
                .to_zset(),
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

    /// The two views of an output agree on the rows and differ in what they
    /// show: the debug view puts the key columns next to the values, the
    /// regular one reports the columns a query sees.
    #[test]
    fn test_output_views() -> Result<(), anyhow::Error> {
        let plan = vec![
            Stmt::from(VarStmt {
                name: "edges".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(EdgeRel::id()))),
            }),
            output_stmt("edges"),
        ];
        let mut rt =
            Pipeline::incremental().runtime(&mut TestProgram::new(plan, [EdgeRel::schema()]))?;
        assert!(rt.feed(&EdgeRel::id(), rows_with_weight([EdgeRel::new(0, 1, 5)], 1))?);
        rt.commit()?;
        let output = rt.output(&SinkId::from("edges"))?;

        // Nothing is hidden in this relation, so both views carry the same rows.
        assert_eq!(output.view().to_zrows().count(), 1);
        assert_eq!(output.view().to_zset(), output.debug_view(false).to_zset());

        // Only the debug table names the key columns.
        let table = output.view().to_cli_table()?.to_string();
        let debug_table = output.debug_view(true).to_cli_table()?.to_string();
        assert!(table.contains("zweight") && !table.contains("[key]"));
        assert!(debug_table.contains("[key] from") && debug_table.contains("[value] weight"));

        Ok(())
    }

    /// Output names must be unique across the whole plan, regardless of kind.
    /// A `Cli` tap and a `Channel` output cannot share a name.
    #[test]
    fn test_duplicate_output_names_rejected() {
        let plan = vec![
            Stmt::from(VarStmt {
                name: "edges".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(EdgeRel::id()))),
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
        let Err(err) =
            Pipeline::incremental().runtime(&mut TestProgram::new(plan, [EdgeRel::schema()]))
        else {
            panic!("duplicate output names must be rejected at build time");
        };
        println!("{err}");
        assert!(
            err.to_string().contains("duplicate output name"),
            "expected a duplicate-name error, got: {err}"
        );
    }

    #[test]
    fn source_no_catalog_describes_is_rejected_at_build_time() {
        // A source leaf only *names* its relation, so a plan can name one the
        // catalog says nothing about. That is caught up front, before the
        // backend builds anything, and the error names the offending source
        // rather than surfacing later as an input that was never wired.
        let plan = vec![
            Stmt::from(VarStmt {
                name: "edges".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(EdgeRel::id()))),
            }),
            output_stmt("edges"),
        ];
        let Err(err) = Pipeline::incremental().runtime(&mut TestProgram::new(plan, [])) else {
            panic!("a source the catalog does not describe must be rejected");
        };
        assert!(
            err.to_string().contains("edge"),
            "expected the error to name the unknown source, got: {err}"
        );
    }

    #[test]
    fn test_standard_join() -> Result<(), anyhow::Error> {
        let plan = vec![
            Stmt::from(VarStmt {
                name: "person".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(PersonRel::id()))),
            }),
            Stmt::from(VarStmt {
                name: "profession".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(ProfessionRel::id()))),
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
        let mut rt = Pipeline::incremental().runtime(&mut TestProgram::new(
            plan,
            [PersonRel::schema(), ProfessionRel::schema()],
        ))?;

        for (person_step, profession_step) in person_profession_data() {
            assert!(rt.feed(&PersonRel::id(), rows_with_weight(person_step, 1))?);
            assert!(rt.feed(&ProfessionRel::id(), rows_with_weight(profession_step, 1),)?);

            rt.commit()?;

            assert_eq!(
                rt.output(&SinkId::from("joined"))?
                    .debug_view(false)
                    .to_zset(),
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
                initializer: Some(Expr::from(SourceExpr::new(PersonRel::id()))),
            }),
            Stmt::from(VarStmt {
                name: "profession".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(ProfessionRel::id()))),
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
        let mut rt = Pipeline::incremental().runtime(&mut TestProgram::new(
            plan,
            [PersonRel::schema(), ProfessionRel::schema()],
        ))?;

        for (person_step, profession_step) in person_profession_data() {
            assert!(rt.feed(&PersonRel::id(), rows_with_weight(person_step, 1))?);
            assert!(rt.feed(&ProfessionRel::id(), rows_with_weight(profession_step, 1),)?);

            rt.commit()?;

            assert_eq!(
                rt.output(&SinkId::from("joined"))?
                    .debug_view(false)
                    .to_zset(),
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
                initializer: Some(Expr::from(SourceExpr::new(EdgeRel::id()))),
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
        let mut rt =
            Pipeline::incremental().runtime(&mut TestProgram::new(plan, [EdgeRel::schema()]))?;

        let init_data = [
            EdgeRel::new(0, 1, 1),
            // This edge is omitted: Edge::new(1, 2, 1),
            EdgeRel::new(2, 3, 2),
            EdgeRel::new(3, 4, 2),
        ];

        assert!(rt.feed(&EdgeRel::id(), rows_with_weight(init_data, 1))?);

        rt.commit()?;

        assert_eq!(
            rt.output(&SinkId::from("full_closure"))?
                .debug_view(false)
                .to_zset(),
            zset! {
                tuple!(0_u64, 1_u64, 1_u64, 1_u64) => 1,
                tuple!(2_u64, 3_u64, 2_u64, 1_u64) => 1,
                tuple!(2_u64, 4_u64, 4_u64, 2_u64) => 1,
                tuple!(3_u64, 4_u64, 2_u64, 1_u64) => 1,
            }
        );

        let extra_data = [EdgeRel::new(1, 2, 1)];

        assert!(rt.feed(&EdgeRel::id(), rows_with_weight(extra_data, 1))?);

        rt.commit()?;

        assert_eq!(
            rt.output(&SinkId::from("full_closure"))?
                .debug_view(false)
                .to_zset(),
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
                    relation: Expr::from(SourceExpr::new(EdgeRel::id())),
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
        let mut rt =
            Pipeline::incremental().runtime(&mut TestProgram::new(plan, [EdgeRel::schema()]))?;

        let init_data = [
            EdgeRel::new(0, 1, 1),
            EdgeRel::new(1, 2, 1),
            EdgeRel::new(2, 3, 2),
            EdgeRel::new(3, 4, 2),
        ];

        assert!(rt.feed(&EdgeRel::id(), rows_with_weight(init_data, 1))?);

        rt.commit()?;

        assert_eq!(
            rt.output(&SinkId::from("closure"))?
                .debug_view(false)
                .to_zset(),
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

    /// Reachability from the seed nodes in `plain` over `edges`,
    /// arithmetic-free. Shared between the incremental run and its
    /// batch twin below.
    fn reachability_plan() -> Vec<Stmt> {
        vec![
            // base = the seed node ids from `plain`, as a single `node` column.
            Stmt::from(VarStmt {
                name: "base".to_string(),
                initializer: Some(Expr::from(ProjectionExpr {
                    relation: Expr::from(SourceExpr::new(PlainRel::id())),
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
                                    relation: Expr::from(SourceExpr::new(EdgeRel::id())),
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
        ]
    }

    #[test]
    fn source_leaf_inside_fixed_point_step_is_bridged() -> Result<(), anyhow::Error> {
        // A `SourceExpr` referenced *only* inside a step body is legal: the
        // backend wires its root input and `delta0`s it into the nested circuit,
        // exactly as it would an outer variable — no explicit imports needed.
        // This computes reachability: starting from the seed nodes in `plain`,
        // follow `edges` transitively. `edges` appears nowhere but the step, so
        // this exercises wiring a source's root input for a step-only source.
        let plan = reachability_plan();

        let mut rt = Pipeline::incremental().runtime(&mut TestProgram::new(
            plan,
            [PlainRel::schema(), EdgeRel::schema()],
        ))?;

        // Seed node 0; edges 0->1->2->3.
        assert!(rt.feed(&PlainRel::id(), rows([(PlainRel::new(0, 0, 0), 1)]),)?);
        assert!(rt.feed(
            &EdgeRel::id(),
            rows_with_weight(
                [
                    EdgeRel::new(0, 1, 1),
                    EdgeRel::new(1, 2, 1),
                    EdgeRel::new(2, 3, 1)
                ],
                1,
            ),
        )?);
        rt.commit()?;

        assert_eq!(
            rt.output(&SinkId::from("reachable"))?
                .debug_view(false)
                .to_zset(),
            zset! {
                tuple!(0_u64) => 1,
                tuple!(1_u64) => 1,
                tuple!(2_u64) => 1,
                tuple!(3_u64) => 1,
            }
        );

        Ok(())
    }

    /// The reachability plan on the batch backend, cross-checked against
    /// the incremental run. The first commit from empty state makes the
    /// incremental delta equal the full state, so both engines must
    /// produce exactly the same rows.
    #[test]
    fn batch_reachability_matches_incremental() -> Result<(), anyhow::Error> {
        let seed = || [(PlainRel::new(0, 0, 0), 1)];
        let edges = || {
            [
                EdgeRel::new(0, 1, 1),
                EdgeRel::new(1, 2, 1),
                EdgeRel::new(2, 3, 1),
            ]
        };
        let schemas = || [PlainRel::schema(), EdgeRel::schema()];

        let mut batch =
            Pipeline::batch().runtime(&mut TestProgram::new(reachability_plan(), schemas()))?;
        assert!(batch.feed(&PlainRel::id(), rows(seed()))?);
        assert!(batch.feed(&EdgeRel::id(), rows_with_weight(edges(), 1))?);
        batch.commit()?;
        let snapshot = batch.output(&SinkId::from("reachable"))?;
        assert_eq!(snapshot.columns(), ["node"]);
        assert_eq!(snapshot.len(), 4);
        assert!(!snapshot.is_empty());

        let mut incremental = Pipeline::incremental()
            .runtime(&mut TestProgram::new(reachability_plan(), schemas()))?;
        assert!(incremental.feed(&PlainRel::id(), rows(seed()))?);
        assert!(incremental.feed(&EdgeRel::id(), rows_with_weight(edges(), 1))?);
        incremental.commit()?;

        let expected = zset! {
            tuple!(0_u64) => 1,
            tuple!(1_u64) => 1,
            tuple!(2_u64) => 1,
            tuple!(3_u64) => 1,
        };
        assert_eq!(snapshot.to_debug_zset(), expected);
        assert_eq!(
            incremental
                .output(&SinkId::from("reachable"))?
                .view()
                .to_zset(),
            expected
        );
        Ok(())
    }

    /// A plain u64 join through the whole batch pipeline, cross-checked
    /// against the incremental backend.
    #[test]
    fn batch_join_matches_incremental() -> Result<(), anyhow::Error> {
        let plan = || {
            vec![
                Stmt::from(VarStmt {
                    name: "edges".to_string(),
                    initializer: Some(Expr::from(SourceExpr::new(EdgeRel::id()))),
                }),
                Stmt::from(VarStmt {
                    name: "two_hops".to_string(),
                    initializer: Some(Expr::from(EquiJoinExpr {
                        left: Expr::from(AliasExpr {
                            relation: Expr::from(VarExpr::new("edges")),
                            alias: "h1".to_string(),
                        }),
                        right: Expr::from(AliasExpr {
                            relation: Expr::from(VarExpr::new("edges")),
                            alias: "h2".to_string(),
                        }),
                        on: vec![(
                            Expr::from(VarExpr::new("to")),
                            Expr::from(VarExpr::new("from")),
                        )],
                        attributes: Some(vec![
                            ("start".to_string(), Expr::from(VarExpr::new("h1.from"))),
                            ("mid".to_string(), Expr::from(VarExpr::new("h1.to"))),
                            ("end".to_string(), Expr::from(VarExpr::new("h2.to"))),
                        ]),
                    })),
                }),
                output_stmt("two_hops"),
            ]
        };
        let data = || {
            [
                EdgeRel::new(0, 1, 1),
                EdgeRel::new(1, 2, 1),
                EdgeRel::new(5, 6, 1),
            ]
        };

        let mut batch =
            Pipeline::batch().runtime(&mut TestProgram::new(plan(), [EdgeRel::schema()]))?;
        assert!(batch.feed(&EdgeRel::id(), rows_with_weight(data(), 1))?);
        batch.commit()?;

        let mut incremental =
            Pipeline::incremental().runtime(&mut TestProgram::new(plan(), [EdgeRel::schema()]))?;
        assert!(incremental.feed(&EdgeRel::id(), rows_with_weight(data(), 1))?);
        incremental.commit()?;

        let expected = zset! { tuple!(0_u64, 1_u64, 2_u64) => 1 };
        assert_eq!(
            batch.output(&SinkId::from("two_hops"))?.to_debug_zset(),
            expected
        );
        assert_eq!(
            incremental
                .output(&SinkId::from("two_hops"))?
                .view()
                .to_zset(),
            expected
        );
        Ok(())
    }

    /// The standard join, persons and professions with string names, on
    /// the batch backend. After every step the full state must equal the
    /// incremental output.
    #[test]
    fn batch_standard_join_matches_incremental() -> Result<(), anyhow::Error> {
        let plan = || {
            vec![
                Stmt::from(VarStmt {
                    name: "person".to_string(),
                    initializer: Some(Expr::from(SourceExpr::new(PersonRel::id()))),
                }),
                Stmt::from(VarStmt {
                    name: "profession".to_string(),
                    initializer: Some(Expr::from(SourceExpr::new(ProfessionRel::id()))),
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
                        on: vec![(
                            Expr::from(VarExpr::new("profession_id")),
                            Expr::from(VarExpr::new("profession_id")),
                        )],
                        attributes: Some(
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
            ]
        };
        let schemas = || [PersonRel::schema(), ProfessionRel::schema()];
        let mut batch = Pipeline::batch().runtime(&mut TestProgram::new(plan(), schemas()))?;
        let mut incremental =
            Pipeline::incremental().runtime(&mut TestProgram::new(plan(), schemas()))?;

        let steps = person_profession_data()
            .into_iter()
            .zip(person_profession_data());
        for ((persons, professions), (persons_again, professions_again)) in steps {
            assert!(batch.feed(&PersonRel::id(), rows_with_weight(persons, 1))?);
            assert!(batch.feed(&ProfessionRel::id(), rows_with_weight(professions, 1))?);
            batch.commit()?;
            assert!(incremental.feed(&PersonRel::id(), rows_with_weight(persons_again, 1))?);
            assert!(
                incremental.feed(&ProfessionRel::id(), rows_with_weight(professions_again, 1))?
            );
            incremental.commit()?;

            let snapshot = batch.output(&SinkId::from("joined"))?;
            assert_eq!(
                snapshot.columns(),
                [
                    "person_id",
                    "person_name",
                    "age",
                    "profession_id",
                    "profession_name"
                ]
            );
            let expected = zset! {
                tuple!(0_u64, "Alice", 20_u64, 0_u64, "Engineer") => 1,
                tuple!(2_u64, "Charlie", 40_u64, 0_u64, "Engineer") => 1,
                tuple!(1_u64, "Bob", 30_u64, 1_u64, "Doctor") => 1,
            };
            assert_eq!(snapshot.to_debug_zset(), expected);
            assert_eq!(
                incremental
                    .output(&SinkId::from("joined"))?
                    .debug_view(false)
                    .to_zset(),
                expected
            );
        }
        Ok(())
    }

    /// Every scalar type the plan knows, except null, flows through feed,
    /// a selection on a string literal, and output unchanged, on both
    /// backends.
    #[test]
    fn batch_carries_every_scalar_type() -> Result<(), anyhow::Error> {
        use crate::api::deltas::ZRow;
        use crate::host::expr::{BinaryExpr, Literal, LiteralExpr};
        use crate::host::operator::Operator;
        use crate::relational::expr::{SelectionExpr, SourceId};
        use crate::relational::schema::{Column, EntityRef, TableSchema};
        use crate::scalarial::ScalarType;

        let schema = || {
            TableSchema::new(
                EntityRef::from("cells"),
                vec![
                    Column::new("id", ScalarType::Uint),
                    Column::new("delta", ScalarType::Iint),
                    Column::new("label", ScalarType::String),
                    Column::new("flag", ScalarType::Bool),
                    Column::new("initial", ScalarType::Char),
                ],
                vec![],
            )
        };
        let plan = || {
            vec![
                Stmt::from(VarStmt {
                    name: "cells".to_string(),
                    initializer: Some(Expr::from(SourceExpr::new(SourceId::from("cells")))),
                }),
                Stmt::from(VarStmt {
                    name: "picked".to_string(),
                    initializer: Some(Expr::from(SelectionExpr {
                        relation: Expr::from(VarExpr::new("cells")),
                        condition: Expr::from(BinaryExpr {
                            operator: Operator::Equal,
                            left: Expr::from(VarExpr::new("label")),
                            right: Expr::from(LiteralExpr {
                                value: Literal::String("b".to_string()),
                            }),
                        }),
                    })),
                }),
                output_stmt("picked"),
            ]
        };
        let data = || {
            [
                tuple!(1_u64, -5_i64, "a", true, 'x'),
                tuple!(2_u64, 7_i64, "b", false, 'y'),
                tuple!(3_u64, -1_i64, "b", true, 'z'),
            ]
            .into_iter()
            .map(|row| ZRow::new(1, row).expect("non-zero zweight"))
            .collect::<Vec<_>>()
        };
        let expected = zset! {
            tuple!(2_u64, 7_i64, "b", false, 'y') => 1,
            tuple!(3_u64, -1_i64, "b", true, 'z') => 1,
        };

        let mut batch = Pipeline::batch().runtime(&mut TestProgram::new(plan(), [schema()]))?;
        assert!(batch.feed(&SourceId::from("cells"), data())?);
        batch.commit()?;
        let snapshot = batch.output(&SinkId::from("picked"))?;
        assert_eq!(
            snapshot.columns(),
            ["id", "delta", "label", "flag", "initial"]
        );
        assert_eq!(snapshot.to_debug_zset(), expected);

        let mut incremental =
            Pipeline::incremental().runtime(&mut TestProgram::new(plan(), [schema()]))?;
        assert!(incremental.feed(&SourceId::from("cells"), data())?);
        incremental.commit()?;
        assert_eq!(
            incremental
                .output(&SinkId::from("picked"))?
                .debug_view(false)
                .to_zset(),
            expected
        );
        Ok(())
    }

    /// Null is the one plan type the batch backend refuses: a null column
    /// fails at build time, a null cell at feed time.
    #[test]
    fn batch_rejects_null() -> Result<(), anyhow::Error> {
        use crate::api::deltas::ZRow;
        use crate::relational::expr::SourceId;
        use crate::relational::schema::{Column, EntityRef, TableSchema};
        use crate::scalarial::{ScalarType, ScalarTypedValue};

        let plan = || {
            vec![
                Stmt::from(VarStmt {
                    name: "t".to_string(),
                    initializer: Some(Expr::from(SourceExpr::new(SourceId::from("t")))),
                }),
                output_stmt("t"),
            ]
        };
        let with_null_column = TableSchema::new(
            EntityRef::from("t"),
            vec![Column::new("n", ScalarType::Null)],
            vec![],
        );
        let err = Pipeline::batch()
            .runtime(&mut TestProgram::new(plan(), [with_null_column]))
            .err()
            .expect("a null column cannot be built");
        assert!(err.to_string().contains("null"), "got: {err}");

        let uint_column = TableSchema::new(
            EntityRef::from("t"),
            vec![Column::new("n", ScalarType::Uint)],
            vec![],
        );
        let mut rt = Pipeline::batch().runtime(&mut TestProgram::new(plan(), [uint_column]))?;
        let null_cell = ZRow::new(1, tuple!(())).expect("non-zero zweight");
        let err = rt.feed(&SourceId::from("t"), [null_cell]).unwrap_err();
        assert!(err.to_string().contains("null"), "got: {err}");
        let _ = ScalarTypedValue::Null(());
        Ok(())
    }

    /// One cell of plan type `ty` for the small integer `x`, injective on
    /// the values these tests use (booleans get `0..2`). Not the identity
    /// for any type, so encoding and decoding are both exercised.
    fn typed_cell(ty: crate::scalarial::ScalarType, x: u64) -> ScalarTypedValue {
        use crate::scalarial::ScalarType;
        match ty {
            ScalarType::Uint => ScalarTypedValue::Uint(u64::MAX - x),
            ScalarType::Iint => ScalarTypedValue::Iint(x as i64 - 5),
            ScalarType::Bool => {
                assert!(x < 2, "boolean instances use the domain 0..2");
                ScalarTypedValue::Bool(x == 1)
            }
            ScalarType::Char => {
                ScalarTypedValue::Char(char::from_u32(0x1F600 + x as u32).expect("emoji range"))
            }
            ScalarType::String => ScalarTypedValue::String(format!("wert {x} ß")),
            ScalarType::Null => unreachable!("null is not a stored type"),
        }
    }

    const STORED_TYPES: [crate::scalarial::ScalarType; 5] = [
        crate::scalarial::ScalarType::Uint,
        crate::scalarial::ScalarType::Iint,
        crate::scalarial::ScalarType::Bool,
        crate::scalarial::ScalarType::Char,
        crate::scalarial::ScalarType::String,
    ];

    /// The table `edge(from: ty, to: ty)`.
    fn typed_edge_schema(
        ty: crate::scalarial::ScalarType,
    ) -> crate::relational::schema::TableSchema {
        use crate::relational::schema::{Column, EntityRef, TableSchema};
        TableSchema::new(
            EntityRef::from("edge"),
            vec![Column::new("from", ty), Column::new("to", ty)],
            vec![],
        )
    }

    fn typed_edges(
        ty: crate::scalarial::ScalarType,
        pairs: &[(u64, u64)],
    ) -> Vec<crate::api::deltas::ZRow> {
        pairs
            .iter()
            .map(|&(a, b)| {
                crate::api::deltas::ZRow::new(
                    1,
                    TupleValue {
                        data: vec![typed_cell(ty, a), typed_cell(ty, b)],
                    },
                )
                .expect("non-zero zweight")
            })
            .collect()
    }

    fn typed_pairs_zset(
        ty: crate::scalarial::ScalarType,
        pairs: &[(u64, u64)],
    ) -> OrdZSet<TupleValue> {
        OrdZSet::from_keys(
            (),
            pairs
                .iter()
                .map(|&(a, b)| {
                    ::dbsp::utils::Tup2(
                        TupleValue {
                            data: vec![typed_cell(ty, a), typed_cell(ty, b)],
                        },
                        1,
                    )
                })
                .collect::<Vec<_>>(),
        )
    }

    fn edge_source() -> Expr {
        use crate::relational::expr::SourceId;
        Expr::from(SourceExpr::new(SourceId::from("edge")))
    }

    fn edge_id() -> crate::relational::expr::SourceId {
        crate::relational::expr::SourceId::from("edge")
    }

    /// Two hops along `edge`: `two_hops(start, end)`.
    fn two_hop_plan() -> Vec<Stmt> {
        vec![
            Stmt::from(VarStmt {
                name: "edges".to_string(),
                initializer: Some(edge_source()),
            }),
            Stmt::from(VarStmt {
                name: "two_hops".to_string(),
                initializer: Some(Expr::from(EquiJoinExpr {
                    left: Expr::from(AliasExpr {
                        relation: Expr::from(VarExpr::new("edges")),
                        alias: "h1".to_string(),
                    }),
                    right: Expr::from(AliasExpr {
                        relation: Expr::from(VarExpr::new("edges")),
                        alias: "h2".to_string(),
                    }),
                    on: vec![(
                        Expr::from(VarExpr::new("to")),
                        Expr::from(VarExpr::new("from")),
                    )],
                    attributes: Some(vec![
                        ("start".to_string(), Expr::from(VarExpr::new("h1.from"))),
                        ("end".to_string(), Expr::from(VarExpr::new("h2.to"))),
                    ]),
                })),
            }),
            output_stmt("two_hops"),
        ]
    }

    /// The transitive closure of `edge` as a fixed point, read through a
    /// distinct so both backends report a set: the batch backend always
    /// does, the incremental one counts derivations otherwise.
    fn closure_plan() -> Vec<Stmt> {
        vec![
            Stmt::from(VarStmt {
                name: "edges".to_string(),
                initializer: Some(edge_source()),
            }),
            Stmt::from(VarStmt {
                name: "closure".to_string(),
                initializer: Some(Expr::from(FixedPointIterExpr {
                    accumulator: ("cur".to_string(), Expr::from(VarExpr::new("edges"))),
                    step: BlockStmt {
                        stmts: vec![Stmt::from(ExprStmt {
                            expr: Expr::from(DistinctExpr {
                                relation: Expr::from(EquiJoinExpr {
                                    left: Expr::from(AliasExpr {
                                        relation: Expr::from(VarExpr::new("cur")),
                                        alias: "walk".to_string(),
                                    }),
                                    right: Expr::from(AliasExpr {
                                        relation: Expr::from(VarExpr::new("edges")),
                                        alias: "step".to_string(),
                                    }),
                                    on: vec![(
                                        Expr::from(VarExpr::new("to")),
                                        Expr::from(VarExpr::new("from")),
                                    )],
                                    attributes: Some(vec![
                                        ("from".to_string(), Expr::from(VarExpr::new("walk.from"))),
                                        ("to".to_string(), Expr::from(VarExpr::new("step.to"))),
                                    ]),
                                }),
                            }),
                        })],
                    },
                })),
            }),
            Stmt::from(VarStmt {
                name: "closure_set".to_string(),
                initializer: Some(Expr::from(DistinctExpr {
                    relation: Expr::from(VarExpr::new("closure")),
                })),
            }),
            output_stmt("closure_set"),
        ]
    }

    fn two_hops_of(pairs: &[(u64, u64)]) -> Vec<(u64, u64)> {
        let mut out: Vec<(u64, u64)> = pairs
            .iter()
            .flat_map(|&(a, b)| {
                pairs
                    .iter()
                    .filter(move |&&(c, _)| c == b)
                    .map(move |&(_, d)| (a, d))
            })
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    fn closure_of(pairs: &[(u64, u64)]) -> Vec<(u64, u64)> {
        let mut closure: Vec<(u64, u64)> = pairs.to_vec();
        loop {
            let mut next = closure.clone();
            next.extend(two_hops_of(&closure));
            next.sort_unstable();
            next.dedup();
            if next.len() == closure.len() {
                return closure;
            }
            closure = next;
        }
    }

    /// The same two-hop plan over every stored type: the batch state and
    /// the incremental output must both equal the expected pairs.
    #[test]
    fn batch_join_every_type_matches_incremental() -> Result<(), anyhow::Error> {
        for ty in STORED_TYPES {
            let pairs: &[(u64, u64)] = if ty == crate::scalarial::ScalarType::Bool {
                &[(0, 1), (1, 1)]
            } else {
                &[(0, 1), (1, 2), (2, 3), (3, 1), (4, 4)]
            };
            let expected = typed_pairs_zset(ty, &two_hops_of(pairs));
            assert!(!expected.is_empty());

            let mut batch = Pipeline::batch().runtime(&mut TestProgram::new(
                two_hop_plan(),
                [typed_edge_schema(ty)],
            ))?;
            assert!(batch.feed(&edge_id(), typed_edges(ty, pairs))?);
            batch.commit()?;
            let snapshot = batch.output(&SinkId::from("two_hops"))?;
            assert_eq!(snapshot.columns(), ["start", "end"]);
            assert_eq!(snapshot.to_debug_zset(), expected, "batch over {ty}");

            let mut incremental = Pipeline::incremental().runtime(&mut TestProgram::new(
                two_hop_plan(),
                [typed_edge_schema(ty)],
            ))?;
            assert!(incremental.feed(&edge_id(), typed_edges(ty, pairs))?);
            incremental.commit()?;
            assert_eq!(
                incremental
                    .output(&SinkId::from("two_hops"))?
                    .debug_view(false)
                    .to_zset(),
                expected,
                "incremental over {ty}"
            );
        }
        Ok(())
    }

    /// The same fixed point over every stored type, on both backends.
    #[test]
    fn batch_fixed_point_every_type_matches_incremental() -> Result<(), anyhow::Error> {
        for ty in STORED_TYPES {
            // A fork (0 → 2 directly and via 1) and a self loop: pairs with
            // several derivations, which only a set semantics reports once.
            let pairs: &[(u64, u64)] = if ty == crate::scalarial::ScalarType::Bool {
                &[(0, 1), (1, 1)]
            } else {
                &[(0, 1), (1, 2), (0, 2), (2, 3), (3, 3)]
            };
            let expected = typed_pairs_zset(ty, &closure_of(pairs));

            let mut batch = Pipeline::batch().runtime(&mut TestProgram::new(
                closure_plan(),
                [typed_edge_schema(ty)],
            ))?;
            assert!(batch.feed(&edge_id(), typed_edges(ty, pairs))?);
            batch.commit()?;
            assert_eq!(
                batch.output(&SinkId::from("closure_set"))?.to_debug_zset(),
                expected,
                "batch over {ty}"
            );

            let mut incremental = Pipeline::incremental().runtime(&mut TestProgram::new(
                closure_plan(),
                [typed_edge_schema(ty)],
            ))?;
            assert!(incremental.feed(&edge_id(), typed_edges(ty, pairs))?);
            incremental.commit()?;
            assert_eq!(
                incremental
                    .output(&SinkId::from("closure_set"))?
                    .debug_view(false)
                    .to_zset(),
                expected,
                "incremental over {ty}"
            );
        }
        Ok(())
    }

    /// A fed cell must have its column's type, and a row its schema's
    /// arity; both mismatches fail at feed time, naming the source.
    #[test]
    fn batch_feed_checks_cell_types() -> Result<(), anyhow::Error> {
        use crate::api::deltas::ZRow;

        let plan = vec![
            Stmt::from(VarStmt {
                name: "people".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(PersonRel::id()))),
            }),
            output_stmt("people"),
        ];
        let mut rt =
            Pipeline::batch().runtime(&mut TestProgram::new(plan, [PersonRel::schema()]))?;

        let wrong_type = ZRow::new(1, tuple!("zero", "Alice", 20_u64, 0_u64)).expect("non-zero");
        let err = rt.feed(&PersonRel::id(), [wrong_type]).unwrap_err();
        assert!(err.to_string().contains("expected uint"), "got: {err}");
        assert!(err.to_string().contains("person"), "got: {err}");

        let wrong_arity = ZRow::new(1, tuple!(0_u64, "Alice")).expect("non-zero");
        let err = rt.feed(&PersonRel::id(), [wrong_arity]).unwrap_err();
        assert!(err.to_string().contains("columns"), "got: {err}");
        Ok(())
    }

    /// Reading an output before any commit is an error, and feeding a
    /// source the plan does not use reports `false` instead of failing.
    #[test]
    fn batch_output_requires_commit() -> Result<(), anyhow::Error> {
        let plan = vec![
            Stmt::from(VarStmt {
                name: "edges".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(EdgeRel::id()))),
            }),
            output_stmt("edges"),
        ];
        let mut rt = Pipeline::batch().runtime(&mut TestProgram::new(plan, [EdgeRel::schema()]))?;
        assert!(!rt.feed(&PlainRel::id(), rows([(PlainRel::new(0, 0, 0), 1)]))?);
        let err = rt.output(&SinkId::from("edges")).unwrap_err();
        assert!(err.to_string().contains("commit"), "got: {err}");
        Ok(())
    }

    #[test]
    fn test_mvr_store_crdt() -> Result<(), anyhow::Error> {
        let plan = vec![
            // Inputs start.
            Stmt::from(VarStmt {
                name: "pred".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(PredRel::id()))),
            }),
            Stmt::from(VarStmt {
                name: "set".to_string(),
                initializer: Some(Expr::from(SourceExpr::new(SetRel::id()))),
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
        let mut rt = Pipeline::incremental().runtime(&mut TestProgram::new(
            plan,
            [PredRel::schema(), SetRel::schema()],
        ))?;

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
            vec![SetRel::new(0, 0, 1, 1)],
            vec![SetRel::new(0, 1, 1, 2), SetRel::new(1, 0, 1, 3)],
            vec![SetRel::new(1, 2, 1, 4)],
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
            assert!(rt.feed(&PredRel::id(), rows_with_weight(pred_rel_step, 1))?);
            assert!(rt.feed(&SetRel::id(), rows_with_weight(set_op_step, 1))?);

            rt.commit()?;

            assert_eq!(
                rt.output(&SinkId::from("mvrStore"))?
                    .debug_view(false)
                    .to_zset(),
                expected.next().unwrap()
            );
        }

        Ok(())
    }
}
