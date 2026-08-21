// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::super::relation::{
    Relation, RelationRef, SchemaTuple, TupleKey, TupleValue, new_relation,
};
use super::operators::{
    coalesce::coalesce_helper,
    projection::{ProjectionStrategy, projection_helper},
    reindex::reindex_helper,
};
use crate::relational::RelationSchema;
use crate::relational::expr::MultiWayEquiJoinExpr;
use crate::relational::incremental::dbsp::{
    DbspInput, OrdIndexedStreamInputHandle, new_ord_indexed_stream,
};
use crate::{
    error::BuildError,
    host::variable::{Value, VariableSlot},
    host::{
        expr::{Expr, VarExpr},
        interpreter::{EvalResult, HostInterpreter, InterpreterContext, assert_type, is_truthy},
        walk::{Node, pre_order},
    },
    relational::expr::{
        AliasExpr, AntiJoinExpr, CartesianProductExpr, DifferenceExpr, DistinctExpr, EquiJoinExpr,
        FixedPointIterExpr, OutputExpr, OutputKind, ProjectionExpr, RelExpr, RelExprVisitor,
        SelectionExpr, SinkId, SourceExpr, UnionExpr,
    },
    relational::incremental::dbsp::{
        DbspError, DbspInputs, DbspOutput, NestedCircuit, OrdIndexedNestedStream, RootCircuit,
        StreamWrapper,
    },
    scalarial::{RowScalarEngine, TreeWalk},
};
use std::collections::HashMap;
use std::{cell::Ref, rc::Rc};

type Sources = HashMap<String, Source>;

struct Source {
    schema: RelationSchema,
    handle: OrdIndexedStreamInputHandle,
    stream: StreamWrapper,
}

impl Source {
    fn from_source_expr(source_expr: &SourceExpr, root_circuit: &mut RootCircuit) -> Self {
        let (stream, handle) = new_ord_indexed_stream(root_circuit);
        Source {
            schema: source_expr.schema.clone(),
            handle,
            stream: StreamWrapper::from(stream),
        }
    }
}

impl From<&Source> for Relation {
    fn from(source: &Source) -> Self {
        Relation::new(source.schema.clone(), source.stream.clone())
    }
}

impl From<Sources> for DbspInputs {
    fn from(sources: Sources) -> Self {
        DbspInputs::from_named_inputs(
            sources
                .into_iter()
                // We drop the stream in `source` because it is !Send to be able
                // to cross the `DbspRuntime::init_circuit` boundary.
                .map(|(name, source)| (name, DbspInput::new(source.schema, source.handle))),
        )
    }
}

type Sinks = Vec<(SinkId, OutputKind, DbspOutput)>;

/// Identifies an outer relation bridged into a fixed-point step, so repeated
/// references share a single `delta0` import node rather than wiring one each.
#[derive(Clone, PartialEq, Eq, Hash)]
enum ImportKey {
    /// An outer relation reached by variable, keyed by its resolved slot.
    Var(VariableSlot),
    /// A [`SourceExpr`] leaf, keyed by source name.
    Source(String),
}

/// State that only exists while walking a [`FixedPointIterExpr`] step body,
/// i.e. while building a nested circuit.
struct StepContext {
    /// The nested circuit that outer relations must be `delta0`'d into. Owned
    /// (cheap `Rc` clone) so it outlives the `recursive` setup closure and is
    /// reachable from [`visit_var_expr`](DbspInterpreter::visit_var_expr).
    nested_circuit: NestedCircuit,
    /// Outer relations already bridged into this step, so each is imported once.
    imports: HashMap<ImportKey, RelationRef>,
}

/// The DBSP (incremental) eval backend. It supplies the relational operators
/// ([`RelExprVisitor`]); the host layer is inherited from [`HostInterpreter`].
pub struct DbspInterpreter<E: RowScalarEngine = TreeWalk> {
    root_circuit: RootCircuit,
    /// The scalar engine driven on the per-tuple hot path (selection conditions,
    /// projection attributes, join keys).
    engine: E,
    /// Live input streams by source name, populated lazily the first time each
    /// [`SourceExpr`] leaf is visited. Serves both deduplication (one stream per
    /// source, however many leaves reference it) and binding.
    sources: Sources,
    /// Output read handles collected while walking the plan, one per
    /// [`OutputExpr`] tap, in plan order. The backend drains these after
    /// interpretation to wire the runtime's named outputs.
    sinks: Sinks,
    /// `Some` while walking a [`FixedPointIterExpr`] step body (a nested
    /// circuit). Outer relations the step reaches — variables from an enclosing
    /// scope or [`SourceExpr`] leaves — arrive as root streams and are
    /// `delta0`'d into the nested circuit on first use; this holds the target
    /// circuit and the import memo. `None` at the top level.
    step: Option<StepContext>,
}

impl<E: RowScalarEngine> DbspInterpreter<E> {
    pub fn new(root_circuit: RootCircuit, engine: E) -> Self {
        Self {
            root_circuit,
            engine,
            sources: Sources::new(),
            sinks: Vec::new(),
            step: None,
        }
    }

    /// Bridge an outer relation into the current step's nested circuit via
    /// `delta0`, memoized by `key` so repeated references reuse one import node.
    /// Must only be called while [`step`](Self::step) is `Some`.
    fn bridge_import(
        &mut self,
        key: ImportKey,
        schema: RelationSchema,
        root_stream: &StreamWrapper,
    ) -> RelationRef {
        let step = self
            .step
            .as_mut()
            .expect("bridge_import called outside a fixed-point step");
        if let Some(bridged) = step.imports.get(&key) {
            return Rc::clone(bridged);
        }
        let bridged = new_relation(schema, root_stream.delta0(&step.nested_circuit));
        step.imports.insert(key, Rc::clone(&bridged));
        bridged
    }

    /// Take the feed handles collected while walking the plan. Called once by
    /// the backend after [`interpret`](crate::host::interpreter::HostInterpreter::interpret).
    pub fn take_inputs(&mut self) -> DbspInputs {
        let sources = std::mem::take(&mut self.sources);
        DbspInputs::from(sources)
    }

    /// Take the output handles collected while walking the plan. Called once by
    /// the backend after [`interpret`](crate::host::interpreter::HostInterpreter::interpret).
    pub fn take_outputs(&mut self) -> Vec<(SinkId, OutputKind, DbspOutput)> {
        std::mem::take(&mut self.sinks)
    }
}

type VisitorCtx<'a, 'b> = &'a mut InterpreterContext<'b>;
type ExprVisitorResult = Result<Value, BuildError>;

impl<E: RowScalarEngine> HostInterpreter for DbspInterpreter<E> {
    /// Bridge from the host layer into the DBSP relational operators.
    fn visit_relational_expr(&mut self, expr: &RelExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        self.visit_rel(expr, ctx)
    }

    /// Read a variable, with one DBSP-specific twist: inside a fixed-point step,
    /// a relation coming from an enclosing scope arrives as a *root* stream and
    /// must be `delta0`'d into the nested circuit before any operator uses it.
    /// Relations defined within the step are already nested streams, and
    /// non-relation values (scalars, functions) are never bridged — so the
    /// root-stream test alone is the precise signal. This is what makes the
    /// step's outer references work without an explicit imports list.
    fn visit_var_expr(&mut self, expr: &VarExpr, ctx: &mut InterpreterContext) -> EvalResult {
        // Tuple-context (field) references are scalars; hand them back verbatim,
        // exactly as the default host implementation does.
        if let Some(value) = ctx.tuple_vars.get(&expr.name) {
            return Ok(Value::from(value.clone()));
        }
        let resolved = *expr
            .resolved
            .as_ref()
            .unwrap_or_else(|| panic!("Unresolved variable '{}'.", expr.name));
        let value = ctx.environment.lookup_var(&resolved).clone();
        // Only relations, only inside a step, and only those still at root level
        // (i.e. from an enclosing scope) need bridging; everything else is
        // returned verbatim.
        if self.step.is_none() {
            return Ok(value);
        }
        let Value::Relation(relation) = &value else {
            return Ok(value);
        };
        let relation = Rc::clone(relation);
        let is_root = matches!(
            relation.borrow().downcast_ref::<StreamWrapper>(),
            StreamWrapper::Root(_)
        );
        if !is_root {
            return Ok(value);
        }
        let borrowed = relation.borrow();
        let schema = borrowed.schema.clone();
        let root = borrowed.downcast_ref::<StreamWrapper>();
        Ok(Value::Relation(self.bridge_import(
            ImportKey::Var(resolved),
            schema,
            root,
        )))
    }
}

impl<E: RowScalarEngine> RelExprVisitor<ExprVisitorResult, VisitorCtx<'_, '_>>
    for DbspInterpreter<E>
{
    fn visit_source_expr(&mut self, expr: &SourceExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        // Wire a fresh root input the first time we meet a source, reusing it
        // for every later leaf naming the same source.
        if !self.sources.contains_key(expr.as_id()) {
            let source = Source::from_source_expr(expr, &mut self.root_circuit);
            self.sources.insert(expr.to_id(), source);
        }
        // Snapshot the root stream + schema, dropping the borrow on `sources`
        // before any `&mut self` call below.
        let (schema, root_stream) = {
            let source = self
                .sources
                .get(expr.as_id())
                .expect("source just wired above");
            (source.schema.clone(), source.stream.clone())
        };
        // Inside a fixed-point step the source is an outer relation and must be
        // `delta0`'d into the nested circuit, just like an outer variable.
        let relation = if self.step.is_some() {
            self.bridge_import(ImportKey::Source(expr.to_id()), schema, &root_stream)
        } else {
            new_relation(schema, root_stream)
        };
        Ok(Value::Relation(relation))
    }

    fn visit_output_expr(&mut self, expr: &OutputExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        // Evaluate the tapped relation, wire an output read handle for it, and
        // return the relation *unchanged* so the tap is transparent to any
        // downstream operator.
        let relation = self
            .visit_expr(&expr.relation, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))?;
        let output = DbspOutput::from(&*relation.borrow());
        self.sinks.push((expr.id.clone(), expr.kind, output));
        Ok(Value::Relation(relation))
    }

    fn visit_alias_expr(&mut self, expr: &AliasExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        ctx.set_alias(expr.alias.clone());
        self.visit_expr(&expr.relation, ctx)
    }

    fn visit_distinct_expr(&mut self, expr: &DistinctExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        let relation = self
            .visit_expr(&expr.relation, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))
            .map(coalesce_helper)?;
        let relation_ref = relation.borrow();

        let distincted = relation_ref.downcast_ref::<StreamWrapper>().distinct();

        Ok(Value::Relation(new_relation(
            relation_ref.schema.clone(),
            distincted,
        )))
    }

    fn visit_union_expr(&mut self, expr: &UnionExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        let relations: Vec<RelationRef> = expr
            .relations
            .iter()
            .map(|relation| {
                self.visit_expr(relation, ctx)
                    .and_then(|value| assert_type!(value, Value::Relation))
                    .map(coalesce_helper)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let relations: Vec<Ref<'_, Relation>> =
            relations.iter().map(|relation| relation.borrow()).collect();

        let (first, others) = relations
            .split_first()
            .expect("Resolver has *not* done its job and ensured that there are at least two operands to a union!");

        let unioned = first.downcast_ref::<StreamWrapper>().sum(
            others
                .iter()
                .map(|relation| relation.downcast_ref::<StreamWrapper>()),
        );

        Ok(Value::Relation(new_relation(first.schema.clone(), unioned)))
    }

    fn visit_difference_expr(
        &mut self,
        expr: &DifferenceExpr,
        ctx: VisitorCtx,
    ) -> ExprVisitorResult {
        let left = self
            .visit_expr(&expr.left, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))
            .map(coalesce_helper)?;
        let right = self
            .visit_expr(&expr.right, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))
            .map(coalesce_helper)?;

        let left_ref = left.borrow();

        let differenced = left_ref
            .downcast_ref::<StreamWrapper>()
            .minus(right.borrow().downcast_ref::<StreamWrapper>());

        Ok(Value::Relation(new_relation(
            left_ref.schema.clone(),
            differenced,
        )))
    }

    fn visit_selection_expr(&mut self, expr: &SelectionExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        let relation = self
            .visit_expr(&expr.relation, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))?;
        let relation_ref = relation.borrow();
        let relation_clone = Rc::clone(&relation);

        let engine = self.engine.clone();
        let condition = engine
            .compile(&expr.condition)
            // TODO: beautify.
            .expect("Condition compilation error");
        let environment = ctx.environment.clone();
        let selected = relation_ref
            .downcast_ref::<StreamWrapper>()
            .filter(move |(_key, tuple)| {
                // No need to run resolver here, already resolved!
                let schema = &relation_clone.borrow().schema;
                let environment = &mut environment.clone();
                let mut new_ctx = InterpreterContext::new(environment);
                new_ctx.extend_tuple_ctx(&None, &schema.tuple, tuple);
                let value = engine
                    .run(&condition, &mut new_ctx)
                    .expect("Runtime error while interpreting selection condition");
                is_truthy(&value)
            });

        Ok(Value::Relation(new_relation(
            relation_ref.schema.select(),
            selected,
        )))
    }

    fn visit_projection_expr(
        &mut self,
        expr: &ProjectionExpr,
        ctx: VisitorCtx,
    ) -> ExprVisitorResult {
        let relation = self
            .visit_expr(&expr.relation, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))?;
        let relation_ref = relation.borrow();

        let (schema, projected) = match projection_helper(&expr.attributes) {
            ProjectionStrategy::Projection(projection) => {
                let (schema, projection) =
                    projection.prepare(&relation_ref.schema, self.engine.clone());
                let projected = relation_ref.downcast_ref::<StreamWrapper>().map_index({
                    let relation_clone = Rc::clone(&relation);
                    let environment = ctx.environment.clone();
                    move |(key, tuple)| {
                        let schema = &relation_clone.borrow().schema;
                        let environment = &mut environment.clone();
                        let mut new_ctx = InterpreterContext::new(environment);
                        new_ctx.extend_tuple_ctx(&None, &schema.tuple, tuple);
                        projection(new_ctx)
                    }
                });
                (schema, projected)
            }
            ProjectionStrategy::Pick(pick) => {
                let schema = pick.prepare(&relation_ref.schema);
                let picked = relation_ref.downcast_ref::<StreamWrapper>().clone();
                (schema, picked)
            }
        };

        Ok(Value::Relation(new_relation(schema, projected)))
    }

    fn visit_cartesian_product_expr(
        &mut self,
        expr: &CartesianProductExpr,
        ctx: VisitorCtx,
    ) -> ExprVisitorResult {
        self.visit_equi_join_expr(&expr.inner, ctx)
    }

    fn visit_equi_join_expr(&mut self, expr: &EquiJoinExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        let left = self
            .visit_expr(&expr.left, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))?;
        // Note the order here. Before we evaluate the right expression,
        // we have to consume the alias of the left relation because it is
        // replaced by the right relation's alias otherwise.
        let left_alias = ctx.consume_alias();

        let right = self
            .visit_expr(&expr.right, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))?;
        let right_alias = ctx.consume_alias();

        let (left_key_fields, right_key_fields): (Vec<&Expr>, Vec<&Expr>) =
            expr.on.iter().map(|(left, right)| (left, right)).unzip();

        let (left_indexed, key_fields) = reindex_helper(
            &left,
            left_key_fields.as_slice(),
            ctx.environment,
            self.engine.clone(),
        );
        let (right_indexed, _) = reindex_helper(
            &right,
            right_key_fields.as_slice(),
            ctx.environment,
            self.engine.clone(),
        );

        let joined_schema = left
            .borrow()
            .schema
            .join(&right.borrow().schema, key_fields);

        let (schema, projection) = match expr
            .attributes
            .as_ref()
            .map(|attributes| projection_helper(attributes))
        {
            Some(ProjectionStrategy::Projection(projection)) => {
                let (projected_schema, projection) =
                    projection.prepare(&joined_schema, self.engine.clone());
                (projected_schema, Some(projection))
            }
            Some(ProjectionStrategy::Pick(pick)) => {
                let picked_schema = pick.prepare(&joined_schema);
                (picked_schema, None)
            }
            None => (joined_schema, None),
        };

        let joined = left_indexed.join_index(&right_indexed, {
            let left_rel = Rc::clone(&left);
            let right_rel = Rc::clone(&right);
            let environment = ctx.environment.clone();
            move |key: &TupleKey, left: &TupleValue, right: &TupleValue| {
                let left_schema = &left_rel.borrow().schema;
                let right_schema = &right_rel.borrow().schema;
                let joined_tuple: TupleValue = SchemaTuple::new(&left_schema.tuple, left)
                    .join(&SchemaTuple::new(&right_schema.tuple, right))
                    .collect();
                let key_tuple_pair = if let Some(projection) = &projection {
                    let environment = &mut environment.clone();
                    let mut new_ctx = InterpreterContext::new(environment);
                    new_ctx.extend_tuple_ctx(&left_alias, &left_schema.tuple, left);
                    new_ctx.extend_tuple_ctx(&right_alias, &right_schema.tuple, right);
                    projection(new_ctx)
                } else {
                    (key.clone(), joined_tuple)
                };
                Some(key_tuple_pair)
            }
        });

        Ok(Value::Relation(new_relation(schema, joined)))
    }

    fn visit_multi_way_equi_join_expr(
        &mut self,
        expr: &MultiWayEquiJoinExpr,
        ctx: VisitorCtx<'_, '_>,
    ) -> ExprVisitorResult {
        // Unreachable through the pipeline: a DBSP circuit joins two streams at
        // a time, which is why `DbspBackend::lower` folds every multi way join
        // into a chain of binary ones before the plan reaches this interpreter.
        // Getting here means the plan skipped that stage.
        unimplemented!(
            "Multi way equi joins are not supported by DBSP. \
             `DbspBackend::lower` (see `relational::incremental::lowering`) folds them \
             into a sequence of binary equi joins; run the plan through `Pipeline` \
             rather than building a circuit from an unlowered plan."
        )
    }

    fn visit_anti_join_expr(&mut self, expr: &AntiJoinExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        let left = self
            .visit_expr(&expr.left, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))?;

        let right = self
            .visit_expr(&expr.right, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))?;

        let (left_key_fields, right_key_fields): (Vec<&Expr>, Vec<&Expr>) =
            expr.on.iter().map(|(left, right)| (left, right)).unzip();

        let (left_indexed, key_fields) = reindex_helper(
            &left,
            left_key_fields.as_slice(),
            ctx.environment,
            self.engine.clone(),
        );
        let (right_indexed, _) = reindex_helper(
            &right,
            right_key_fields.as_slice(),
            ctx.environment,
            self.engine.clone(),
        );

        let anti_joined_schema = left
            .borrow()
            .schema
            .anti_join(&right.borrow().schema, key_fields);
        let anti_joined = left_indexed.anti_join_index(&right_indexed);

        Ok(Value::Relation(new_relation(
            anti_joined_schema,
            anti_joined,
        )))
    }

    fn visit_fixed_point_iter_expr(
        &mut self,
        expr: &FixedPointIterExpr,
        ctx: VisitorCtx,
    ) -> ExprVisitorResult {
        // [`StreamWrapper`] is single-level, so a fixed point nested inside
        // another cannot be represented (it would need a twice-`delta0`'d
        // stream). Reject it with a clear error rather than panic on the second
        // `delta0`.
        if self.step.is_some() {
            return Err(BuildError::new(
                "nested fixed-point iterations are not supported.",
            ));
        }

        let accumulator = self
            .visit_expr(&expr.accumulator.1, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))
            .map(coalesce_helper)?;

        let (accumulator_init, schema) = {
            let accumulator = accumulator.borrow();
            (
                accumulator.downcast_ref::<StreamWrapper>().clone(),
                accumulator.schema.clone(),
            )
        };

        // Wire the root inputs of any source referenced inside the step *before*
        // entering `recursive`: DBSP forbids adding a root input once a nested
        // circuit is under construction (the input node would not belong to the
        // root scope). Deduped against sources already wired elsewhere; inside
        // the step each is `delta0`'d like any outer relation.
        for source_expr in pre_order(&expr.step.stmts).filter_map(Node::as_source) {
            if !self.sources.contains_key(source_expr.as_id()) {
                let source = Source::from_source_expr(source_expr, &mut self.root_circuit);
                self.sources.insert(source_expr.to_id(), source);
            }
        }

        pre_order(&expr.step.stmts)
            .filter_map(Node::as_output)
            .next()
            .map_or(Ok(()), |_output_expr| {
                Err(BuildError::new(
                    "a fix point's step body must not contain output expressions",
                ))
            })?;

        // A build error raised while walking the step body cannot be returned
        // through `recursive`'s closure (its error channel is DBSP's
        // `SchedulerError`), so stash it here and propagate it once the circuit
        // is closed.
        let mut step_error: Option<BuildError> = None;
        let root_circuit = self.root_circuit.clone();
        let accumulated = root_circuit
            .recursive(|nested_circuit, acc: OrdIndexedNestedStream| {
                // Enter the step: any outer relation the body reaches (an outer
                // variable or a `SourceExpr`) is now `delta0`'d into this nested
                // circuit on first use — see `visit_var_expr`/`visit_source_expr`.
                self.step = Some(StepContext {
                    nested_circuit: nested_circuit.clone(),
                    imports: HashMap::new(),
                });
                let acc_for_step = acc.clone();
                let accumulator_rel = Rc::clone(&accumulator);
                let nested_for_setup = nested_circuit.clone();
                let result = self.visit_block(&expr.step.stmts, ctx, move |environment| {
                    // The accumulator is the sole step-local binding: its initial
                    // value `delta0`'d in, plus the recursive feedback. delta0
                    // does not alter the schema.
                    let accumulator = accumulator_rel.borrow();
                    let schema = accumulator.schema.clone();
                    let accumulator = accumulator
                        .downcast_ref::<StreamWrapper>()
                        .delta0(&nested_for_setup)
                        .plus(&acc_for_step.into());
                    environment.define_var(new_relation(schema, accumulator));
                });
                // Leave the step before propagating so the state is correct for
                // any sibling code walked after this fixed point.
                self.step = None;
                // Reduce the step's result to the nested stream `recursive`
                // expects. The output is fed into a union below, which requires
                // the schema to be coalesced.
                let stream = result.and_then(|value| {
                    let value = value.ok_or_else(|| {
                        BuildError::new("Fixed point iteration body did not return a value.")
                    })?;
                    let relation = assert_type!(value, Value::Relation).map(coalesce_helper)?;
                    Ok(relation
                        .borrow()
                        .downcast_ref::<StreamWrapper>()
                        .expect_nested()
                        .clone())
                });
                match stream {
                    Ok(stream) => Ok(stream),
                    Err(error) => {
                        // Stash the real cause and hand back the prior
                        // accumulator as an inert placeholder; we bail out right
                        // after `recursive` returns.
                        step_error = Some(error);
                        Ok(acc)
                    }
                }
            })
            .map_err(|error| BuildError::from(DbspError::from(error)))?;

        if let Some(error) = step_error {
            return Err(error);
        }

        let fixed_point = accumulator_init.plus(&accumulated.into());

        Ok(Value::Relation(new_relation(schema, fixed_point)))
    }
}
