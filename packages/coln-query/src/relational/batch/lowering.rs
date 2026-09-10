// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Lowering from the logical plan to coln-batch's physical form.
//!
//! The whole statement list becomes one Datalog program: every bound
//! variable turns into a derived relation defined by one or more rules,
//! sources stay stored relations, and a `FixedPointIter` contributes
//! recursive rules for its accumulator. Evaluating the program with
//! coln-batch's semi-naive fixpoint then computes the entire plan,
//! including the dependencies between statements, in one run.
//!
//! Types travel with the columns. A source column has the type its
//! schema declares, a rule variable the type of the column it was created
//! for, and a derived relation the types of its head. Unifying two
//! columns of different types (a join, a selection against a literal of
//! the wrong type) is a lowering error, so the engine never sees an
//! ill-typed program.
//!
//! Scope of this slice: purely relational plans. Sources, equi joins on
//! columns (chains are flattened into one n-ary rule body), multi-way
//! equi joins (FLIR's native n-ary node, consumed directly), cartesian
//! products, projections onto columns and literals, equality selections
//! against literals, unions, distinct, and fixed points whose step is a
//! single relational expression. Everything that needs the scalar engine
//! (computed columns, general conditions) and the remaining operators
//! (anti join, difference) fail with a clear error instead of a wrong
//! answer.
//!
//! The tests at the bottom build up in difficulty and double as a guided
//! tour of the translation.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use coln_batch::query::{Atom, Term};
use coln_batch::rule::{Program, Rule};
use coln_batch::types::{Column, ScalarType, Schema, Value};

use super::values::{batch_schema, literal_value};
use crate::host::QueryIr;
use crate::host::expr::Expr;
use crate::host::operator::Operator;
use crate::host::stmt::{Stmt, VarStmt};
use crate::relational::catalog::SourceSchemas;
use crate::relational::expr::{
    EquiJoinExpr, FixedPointIterExpr, MultiWayEquiJoinExpr, OutputKind, RelExpr, SourceExpr,
};

/// The result of lowering a logical plan.
#[derive(Debug)]
pub struct LoweredPlan {
    /// One Datalog program covering every statement of the plan.
    pub program: Program,
    /// Source id (schema name) to its schema, in tuple order.
    pub sources: HashMap<String, Schema>,
    /// Sink id to the derived relation `output` reads.
    pub outputs: HashMap<String, String>,
    /// Schema per relation, sources and derived alike.
    pub schemas: HashMap<String, Schema>,
}

/// Lower a plan (its statement list) into a [`LoweredPlan`]. `sources`
/// describes the base tables the plan's [`SourceExpr`] leaves name: the
/// batch projection of a [`Catalog`](crate::relational::catalog::Catalog).
pub fn lower(ir: &QueryIr, sources: &SourceSchemas) -> Result<LoweredPlan> {
    let available_sources = sources
        .iter()
        .map(|(id, schema)| {
            let schema = batch_schema(schema)
                .with_context(|| format!("in the schema of source {}", id.as_str()))?;
            Ok((id.as_str().to_string(), schema))
        })
        .collect::<Result<HashMap<_, _>>>()?;
    let mut lowerer = Lowerer {
        available_sources,
        ..Lowerer::default()
    };
    lowerer.lower_stmts(ir)?;
    let mut schemas: HashMap<String, Schema> = lowerer.sources.clone();
    for (name, info) in &lowerer.env {
        schemas.insert(name.clone(), info.schema.clone());
    }
    Ok(LoweredPlan {
        program: Program {
            rules: lowerer.rules,
        },
        sources: lowerer.sources,
        outputs: lowerer.outputs,
        schemas,
    })
}

/// What a plan variable is bound to: a relation plus its schema.
#[derive(Clone)]
struct RelInfo {
    relation: String,
    schema: Schema,
}

/// A column binding inside one rule body under construction.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Bind {
    Var(usize),
    Lit(Value),
}

impl Bind {
    fn term(&self, remap: &HashMap<usize, usize>) -> Result<Term> {
        match self {
            Bind::Var(v) => remap
                .get(v)
                .map(|nv| Term::Var(*nv))
                .context("head column refers to a variable that does not occur in the body"),
            Bind::Lit(value) => Ok(Term::Lit(value.clone())),
        }
    }
}

/// The visible columns of a lowered sub-expression: the output arity in
/// order, plus a lookup table that also knows alias-qualified names.
/// `None` marks a plain name that became ambiguous after a join.
#[derive(Clone, Default)]
struct Scope {
    columns: Vec<(String, Bind)>,
    names: HashMap<String, Option<Bind>>,
}

impl Scope {
    fn resolve(&self, name: &str) -> Result<Bind> {
        match self.names.get(name) {
            Some(Some(bind)) => Ok(bind.clone()),
            Some(None) => bail!("column {name} is ambiguous here, qualify it with an alias"),
            None => bail!("unknown column {name}"),
        }
    }

    fn substitute(&mut self, from: usize, to: &Bind) {
        for (_, bind) in &mut self.columns {
            if *bind == Bind::Var(from) {
                *bind = to.clone();
            }
        }
        for bind in self.names.values_mut() {
            if *bind == Some(Bind::Var(from)) {
                *bind = Some(to.clone());
            }
        }
    }

    /// The columns as a schema, with their types taken from `frame`.
    fn schema(&self, frame: &Frame) -> Schema {
        self.columns
            .iter()
            .map(|(name, bind)| Column::new(name.clone(), frame.ty(bind)))
            .collect()
    }
}

/// One rule body under construction: the variables (each with its type)
/// and the atoms.
#[derive(Default)]
struct Frame {
    var_types: Vec<ScalarType>,
    atoms: Vec<Atom>,
}

impl Frame {
    fn fresh(&mut self, ty: ScalarType) -> usize {
        self.var_types.push(ty);
        self.var_types.len() - 1
    }

    fn ty(&self, bind: &Bind) -> ScalarType {
        match bind {
            Bind::Var(v) => self.var_types[*v],
            Bind::Lit(value) => value.scalar_type(),
        }
    }

    /// Fail unless both bindings have the same type; `what` names the
    /// construct for the message.
    fn check_same_type(&self, left: &Bind, right: &Bind, what: &str) -> Result<()> {
        let (l, r) = (self.ty(left), self.ty(right));
        if l != r {
            bail!("{what} equates a {l} column with a {r} column");
        }
        Ok(())
    }

    fn substitute(&mut self, from: usize, to: &Bind) {
        let to = match to {
            Bind::Var(v) => Term::Var(*v),
            Bind::Lit(value) => Term::Lit(value.clone()),
        };
        for atom in &mut self.atoms {
            for term in &mut atom.terms {
                if *term == Term::Var(from) {
                    *term = to.clone();
                }
            }
        }
    }
}

#[derive(Default)]
struct Lowerer {
    rules: Vec<Rule>,
    /// All base tables the catalog offers (id, schema).
    available_sources: HashMap<String, Schema>,
    /// The subset of `available_sources` the plan actually uses.
    sources: HashMap<String, Schema>,
    env: HashMap<String, RelInfo>,
    outputs: HashMap<String, String>,
}

impl Lowerer {
    fn lower_stmts(&mut self, stmts: &[Stmt]) -> Result<()> {
        for stmt in stmts {
            match stmt {
                Stmt::Var(var) => self.lower_var_stmt(var)?,
                Stmt::Expr(expr_stmt) => self.lower_output_stmt(&expr_stmt.expr)?,
                Stmt::Block(_) => bail!("batch lowering does not support nested blocks yet"),
            }
        }
        Ok(())
    }

    /// A variable binding defines a derived relation named after the
    /// variable, filled by one rule per union branch (usually one).
    fn lower_var_stmt(&mut self, var: &VarStmt) -> Result<()> {
        let initializer = var
            .initializer
            .as_ref()
            .with_context(|| format!("variable {} has no initializer", var.name))?;

        let schema = match initializer {
            // Aliasing one relation variable to another.
            Expr::Var(inner) => {
                let info = self.lookup(&inner.name)?.clone();
                self.env.insert(var.name.clone(), info.clone());
                return Ok(());
            }
            Expr::Relational(rel) => match rel {
                RelExpr::FixedPointIter(fixed_point) => {
                    self.lower_fixed_point(&var.name, fixed_point)?
                }
                RelExpr::Union(union) => {
                    let mut schema: Option<Schema> = None;
                    for branch in &union.relations {
                        let branch_schema = self.push_rule_for(&var.name, branch)?;
                        match &schema {
                            None => schema = Some(branch_schema),
                            Some(first) => {
                                if first.types() != branch_schema.types() {
                                    bail!(
                                        "union branches for {} have different schemas, \
                                         {} and {}",
                                        var.name,
                                        describe(first),
                                        describe(&branch_schema)
                                    );
                                }
                            }
                        }
                    }
                    schema.with_context(|| format!("union for {} is empty", var.name))?
                }
                _ => self.push_rule_for(&var.name, initializer)?,
            },
            _ => bail!(
                "batch lowering only supports relational initializers, {} is a host expression",
                var.name
            ),
        };

        self.env.insert(
            var.name.clone(),
            RelInfo {
                relation: var.name.clone(),
                schema,
            },
        );
        Ok(())
    }

    /// Lower one relational expression into a single rule with the given
    /// head relation; returns the head's schema.
    fn push_rule_for(&mut self, head: &str, expr: &Expr) -> Result<Schema> {
        let mut frame = Frame::default();
        let scope = self.lower_rel(expr, &mut frame)?;
        let schema = scope.schema(&frame);
        self.push_rule(head, frame, &scope)?;
        Ok(schema)
    }

    /// A fixed point defines its accumulator as a recursive relation:
    /// one base rule from the initializer, one rule per step statement,
    /// all with the accumulator as their head. Semi-naive evaluation in
    /// coln-batch then iterates them to the fixpoint.
    fn lower_fixed_point(
        &mut self,
        head: &str,
        fixed_point: &FixedPointIterExpr,
    ) -> Result<Schema> {
        let (accumulator, init) = (&fixed_point.accumulator.0, &fixed_point.accumulator.1);
        let schema = self.push_rule_for(head, init)?;

        // Inside the step, the accumulator name refers to the relation
        // being defined; that reference is what makes the rule recursive.
        let previous = self.env.insert(
            accumulator.clone(),
            RelInfo {
                relation: head.to_string(),
                schema: schema.clone(),
            },
        );

        let result = (|| {
            let [step] = fixed_point.step.stmts.as_slice() else {
                bail!("batch lowering expects exactly one statement in a fixed point step");
            };
            let Stmt::Expr(step_expr) = step else {
                bail!("batch lowering expects the fixed point step to be an expression");
            };
            let step_schema = self.push_rule_for(head, &step_expr.expr)?;
            if step_schema.types() != schema.types() {
                bail!(
                    "fixed point step for {head} has schema {}, its base has {}",
                    describe(&step_schema),
                    describe(&schema)
                );
            }
            Ok(())
        })();

        match previous {
            Some(info) => {
                self.env.insert(accumulator.clone(), info);
            }
            None => {
                self.env.remove(accumulator);
            }
        }
        result?;
        Ok(schema)
    }

    /// An output statement names a derived relation as a sink.
    fn lower_output_stmt(&mut self, expr: &Expr) -> Result<()> {
        let Expr::Relational(rel) = expr else {
            bail!("batch lowering only supports output statements at the top level");
        };
        let RelExpr::Output(output) = rel else {
            bail!("batch lowering only supports output statements at the top level");
        };
        let Expr::Var(var) = &output.relation else {
            bail!("batch lowering expects outputs to tap a bound variable");
        };
        let info = self.lookup(&var.name)?;
        match output.kind {
            OutputKind::Channel => {
                self.outputs
                    .insert(output.id.0.clone(), info.relation.clone());
            }
            // Print-only taps have no readable result; nothing to record.
            OutputKind::Cli => {}
        }
        Ok(())
    }

    /// Lower a relational expression into atoms of `frame`, returning the
    /// visible columns.
    fn lower_rel(&mut self, expr: &Expr, frame: &mut Frame) -> Result<Scope> {
        match expr {
            Expr::Var(var) => {
                let info = self.lookup(&var.name)?.clone();
                Ok(self.scope_from_atom(&info, frame))
            }
            Expr::Relational(rel) => match rel {
                RelExpr::Source(source) => self.lower_source(source, frame),
                RelExpr::Alias(alias) => {
                    let inner = self.lower_rel(&alias.relation, frame)?;
                    let mut names = inner.names.clone();
                    for (name, bind) in &inner.columns {
                        names.insert(format!("{}.{}", alias.alias, name), Some(bind.clone()));
                    }
                    Ok(Scope {
                        columns: inner.columns,
                        names,
                    })
                }
                // coln-batch results are sets already, distinct is free.
                RelExpr::Distinct(distinct) => self.lower_rel(&distinct.relation, frame),
                RelExpr::Projection(projection) => {
                    let inner = self.lower_rel(&projection.relation, frame)?;
                    let mut columns = Vec::with_capacity(projection.attributes.len());
                    let mut names = HashMap::new();
                    for (name, attribute) in &projection.attributes {
                        let bind = self
                            .lower_attribute(attribute, &inner)
                            .with_context(|| format!("in the projection attribute {name}"))?;
                        columns.push((name.clone(), bind.clone()));
                        names.insert(name.clone(), Some(bind));
                    }
                    Ok(Scope { columns, names })
                }
                RelExpr::Selection(selection) => {
                    let mut inner = self.lower_rel(&selection.relation, frame)?;
                    self.apply_selection(&selection.condition, frame, &mut inner)?;
                    Ok(inner)
                }
                RelExpr::EquiJoin(join) => self.lower_equi_join(join, frame),
                RelExpr::CartesianProduct(product) => self.lower_equi_join(&product.inner, frame),
                RelExpr::Union(_) => {
                    bail!("batch lowering supports union only as a direct variable initializer")
                }
                RelExpr::FixedPointIter(_) => {
                    bail!(
                        "batch lowering supports a fixed point only as a direct variable initializer"
                    )
                }
                RelExpr::Output(_) => bail!("outputs cannot be nested inside other operators"),
                RelExpr::MultiWayEquiJoin(join) => self.lower_multi_way(join, frame),
                RelExpr::AntiJoin(_) => bail!("batch lowering does not support AntiJoin yet"),
                RelExpr::Difference(_) => bail!("batch lowering does not support Difference yet"),
            },
            _ => bail!("batch lowering expects a relational expression here"),
        }
    }

    fn lower_source(&mut self, source: &SourceExpr, frame: &mut Frame) -> Result<Scope> {
        let id = source.as_id().as_str().to_string();
        let schema = self
            .available_sources
            .get(&id)
            .with_context(|| format!("source {id} is not in the catalog"))?
            .clone();
        self.sources.insert(id.clone(), schema.clone());
        let info = RelInfo {
            relation: id,
            schema,
        };
        Ok(self.scope_from_atom(&info, frame))
    }

    /// Place one atom over `info`'s relation with fresh variables, one per
    /// column, typed like the column.
    fn scope_from_atom(&mut self, info: &RelInfo, frame: &mut Frame) -> Scope {
        let arity = info.schema.arity();
        let mut columns = Vec::with_capacity(arity);
        let mut names = HashMap::new();
        let mut terms = Vec::with_capacity(arity);
        for column in info.schema.columns() {
            let var = frame.fresh(column.ty);
            terms.push(Term::Var(var));
            columns.push((column.name.clone(), Bind::Var(var)));
            names.insert(column.name.clone(), Some(Bind::Var(var)));
        }
        frame.atoms.push(Atom {
            relation: info.relation.clone(),
            terms,
        });
        Scope { columns, names }
    }

    /// Both join operands are lowered into the same frame; every `on`
    /// pair then unifies one variable of each side, which is exactly how
    /// a conjunctive query expresses an equi join. Chained joins land in
    /// the same frame too, so a chain flattens into one n-ary body and
    /// the executor picks its own order. Kept for frontends that emit
    /// binary chains; FLIR's native n-ary join takes
    /// [`Self::lower_multi_way`].
    fn lower_equi_join(&mut self, join: &EquiJoinExpr, frame: &mut Frame) -> Result<Scope> {
        let left = self.lower_rel(&join.left, frame)?;
        let mut right = self.lower_rel(&join.right, frame)?;
        let mut left = left;

        for (left_expr, right_expr) in &join.on {
            let left_name = Self::column_name(left_expr)?;
            let right_name = Self::column_name(right_expr)?;
            let left_bind = left.resolve(left_name)?;
            let right_bind = right.resolve(right_name)?;
            frame.check_same_type(
                &left_bind,
                &right_bind,
                &format!("join condition {left_name} = {right_name}"),
            )?;
            match (&left_bind, &right_bind) {
                (Bind::Var(l), _) => {
                    frame.substitute(*l, &right_bind);
                    left.substitute(*l, &right_bind);
                }
                (Bind::Lit(_), Bind::Var(r)) => {
                    frame.substitute(*r, &left_bind);
                    right.substitute(*r, &left_bind);
                }
                (Bind::Lit(a), Bind::Lit(b)) if a == b => {}
                (Bind::Lit(a), Bind::Lit(b)) => {
                    bail!("join condition compares two different literals, {a} and {b}")
                }
            }
        }

        let mut names = left.names.clone();
        for (name, bind) in &right.names {
            match names.get(name) {
                // A plain name on both sides becomes ambiguous.
                Some(_) => {
                    names.insert(name.clone(), None);
                }
                None => {
                    names.insert(name.clone(), bind.clone());
                }
            }
        }
        let merged = Scope {
            columns: Vec::new(),
            names,
        };

        let columns = match &join.attributes {
            Some(attributes) => {
                let mut columns = Vec::with_capacity(attributes.len());
                for (name, attribute) in attributes {
                    let bind = self
                        .lower_attribute(attribute, &merged)
                        .with_context(|| format!("in the join attribute {name}"))?;
                    columns.push((name.clone(), bind));
                }
                columns
            }
            None => left
                .columns
                .iter()
                .chain(right.columns.iter())
                .cloned()
                .collect(),
        };

        let mut names = merged.names;
        for (name, bind) in &columns {
            names.insert(name.clone(), Some(bind.clone()));
        }
        Ok(Scope { columns, names })
    }

    /// The n-ary join FLIR emits, consumed natively: every participant
    /// is lowered into the same frame, and each join variable unifies
    /// one column per occurrence. This is the direct construction of an
    /// n-ary rule body, no flattening involved. The output schema
    /// follows the documented left-to-right fold: a join variable
    /// appears once, named after the variable and carried by the first
    /// relation binding it, and a later column is dropped when an
    /// earlier relation already contributes an active column of the
    /// same name.
    fn lower_multi_way(&mut self, join: &MultiWayEquiJoinExpr, frame: &mut Frame) -> Result<Scope> {
        let mut scopes = Vec::with_capacity(join.relations.len());
        for relation in &join.relations {
            scopes.push(self.lower_rel(relation, frame)?);
        }

        // Unify each join variable's occurrences (the constructor
        // guarantees at least two, in relation order) and remember which
        // column carries the variable (first occurrence) and which ones
        // it deactivates (the rest).
        let mut carried: HashMap<(usize, String), Option<String>> = HashMap::new();
        for variable in &join.on {
            let mut kept: Option<Bind> = None;
            for (relation, occurrence) in &variable.occurrences {
                let column = Self::column_name(occurrence)
                    .with_context(|| format!("in join variable {}", variable.name))?
                    .to_string();
                let bind = scopes[*relation]
                    .resolve(&column)
                    .with_context(|| format!("in join variable {}", variable.name))?;
                kept = Some(match kept {
                    None => {
                        carried.insert((*relation, column), Some(variable.name.clone()));
                        bind
                    }
                    Some(kept) => {
                        carried.insert((*relation, column), None);
                        frame.check_same_type(
                            &kept,
                            &bind,
                            &format!("join variable {}", variable.name),
                        )?;
                        match (&kept, &bind) {
                            (Bind::Var(l), _) => {
                                frame.substitute(*l, &bind);
                                for scope in &mut scopes {
                                    scope.substitute(*l, &bind);
                                }
                                bind
                            }
                            (Bind::Lit(_), Bind::Var(r)) => {
                                frame.substitute(*r, &kept);
                                for scope in &mut scopes {
                                    scope.substitute(*r, &kept);
                                }
                                kept
                            }
                            (Bind::Lit(a), Bind::Lit(b)) if a == b => kept,
                            (Bind::Lit(a), Bind::Lit(b)) => bail!(
                                "join variable {} equates two different literals, {a} and {b}",
                                variable.name
                            ),
                        }
                    }
                });
            }
        }

        // The schema fold, left to right over the participants.
        let mut columns: Vec<(String, Bind)> = Vec::new();
        for (relation, scope) in scopes.iter().enumerate() {
            for (name, bind) in &scope.columns {
                match carried.get(&(relation, name.clone())) {
                    Some(Some(variable_name)) => {
                        columns.push((variable_name.clone(), bind.clone()));
                    }
                    Some(None) => {}
                    None => {
                        if columns.iter().all(|(active, _)| active != name) {
                            columns.push((name.clone(), bind.clone()));
                        }
                    }
                }
            }
        }

        // Name lookup for a projection step: alias-qualified names from
        // the participants (first one wins), plain names from the fold.
        let mut names: HashMap<String, Option<Bind>> = HashMap::new();
        for scope in &scopes {
            for (name, bind) in &scope.names {
                if name.contains('.') {
                    names.entry(name.clone()).or_insert(bind.clone());
                }
            }
        }
        for (name, bind) in &columns {
            names.insert(name.clone(), Some(bind.clone()));
        }
        let merged = Scope {
            columns: columns.clone(),
            names,
        };

        let columns = match &join.attributes {
            Some(attributes) => {
                let mut projected = Vec::with_capacity(attributes.len());
                for (name, attribute) in attributes {
                    let bind = self
                        .lower_attribute(attribute, &merged)
                        .with_context(|| format!("in the join attribute {name}"))?;
                    projected.push((name.clone(), bind));
                }
                projected
            }
            None => columns,
        };

        let mut names = merged.names;
        for (name, bind) in &columns {
            names.insert(name.clone(), Some(bind.clone()));
        }
        Ok(Scope { columns, names })
    }

    /// The attributes this slice supports: a column reference or a
    /// literal. Computed attributes arrive with the scalar engine.
    fn lower_attribute(&self, attribute: &Expr, scope: &Scope) -> Result<Bind> {
        match attribute {
            Expr::Var(var) => scope.resolve(&var.name),
            Expr::Literal(literal) => Ok(Bind::Lit(literal_value(&literal.value)?)),
            _ => bail!(
                "computed attributes need the scalar engine and are not supported in this slice"
            ),
        }
    }

    /// The conditions this slice supports: column equals literal (either
    /// side). Everything else needs the scalar engine.
    fn apply_selection(
        &self,
        condition: &Expr,
        frame: &mut Frame,
        scope: &mut Scope,
    ) -> Result<()> {
        let Expr::Binary(binary) = condition else {
            bail!("selection conditions beyond column = literal need the scalar engine");
        };
        if binary.operator != Operator::Equal {
            bail!("selection conditions beyond column = literal need the scalar engine");
        }
        let (column, literal) = match (&binary.left, &binary.right) {
            (Expr::Var(var), Expr::Literal(lit)) => (&var.name, literal_value(&lit.value)?),
            (Expr::Literal(lit), Expr::Var(var)) => (&var.name, literal_value(&lit.value)?),
            _ => bail!("selection conditions beyond column = literal need the scalar engine"),
        };
        let bind = scope.resolve(column)?;
        let column_type = frame.ty(&bind);
        if column_type != literal.scalar_type() {
            bail!(
                "selection compares column {column} ({column_type}) with the {} literal {literal}",
                literal.scalar_type()
            );
        }
        match bind {
            Bind::Var(v) => {
                let pinned = Bind::Lit(literal);
                frame.substitute(v, &pinned);
                scope.substitute(v, &pinned);
            }
            Bind::Lit(existing) if existing == literal => {}
            Bind::Lit(existing) => {
                bail!("column {column} is already pinned to {existing}, cannot equal {literal}")
            }
        }
        Ok(())
    }

    /// Compact the frame's variables to a dense range and emit the rule.
    fn push_rule(&mut self, head_relation: &str, frame: Frame, scope: &Scope) -> Result<()> {
        let mut remap: HashMap<usize, usize> = HashMap::new();
        for atom in &frame.atoms {
            for term in &atom.terms {
                if let Term::Var(v) = term {
                    let next = remap.len();
                    remap.entry(*v).or_insert(next);
                }
            }
        }
        let body = frame
            .atoms
            .into_iter()
            .map(|atom| Atom {
                relation: atom.relation,
                terms: atom
                    .terms
                    .into_iter()
                    .map(|term| match term {
                        Term::Var(v) => Term::Var(remap[&v]),
                        lit => lit,
                    })
                    .collect(),
            })
            .collect();
        let head_terms = scope
            .columns
            .iter()
            .map(|(name, bind)| bind.term(&remap).with_context(|| format!("column {name}")))
            .collect::<Result<Vec<_>>>()?;
        self.rules.push(Rule {
            var_names: (0..remap.len()).map(|i| format!("v{i}")).collect(),
            head: Atom {
                relation: head_relation.to_string(),
                terms: head_terms,
            },
            body,
        });
        Ok(())
    }

    fn lookup(&self, name: &str) -> Result<&RelInfo> {
        self.env
            .get(name)
            .with_context(|| format!("unknown plan variable {name}"))
    }

    fn column_name(expr: &Expr) -> Result<&str> {
        match expr {
            Expr::Var(var) => Ok(&var.name),
            _ => bail!("join conditions must reference columns by name"),
        }
    }
}

/// A schema for error messages: `(name: type, …)`.
fn describe(schema: &Schema) -> String {
    let columns: Vec<String> = schema
        .columns()
        .iter()
        .map(|c| format!("{}: {}", c.name, c.ty))
        .collect();
    format!("({})", columns.join(", "))
}

#[cfg(test)]
mod tests {
    //! A guided tour of the translation, from a single source scan to a
    //! recursive fixed point. Read the tests in order; each one adds a
    //! single concept on top of the previous.

    use super::*;
    use crate::host::expr::{BinaryExpr, Literal, LiteralExpr, VarExpr};
    use crate::host::stmt::{BlockStmt, ExprStmt, VarStmt};
    use crate::relational::expr::{
        AliasExpr, AntiJoinExpr, CartesianProductExpr, DistinctExpr, JoinVariable, OutputExpr,
        ProjectionExpr, SelectionExpr, SinkId, SourceId, UnionExpr,
    };
    use crate::relational::schema::{Column as PlanColumn, EntityRef, TableSchema};
    use crate::scalarial::ScalarType as PlanType;
    use coln_batch::fixpoint::{self, Exec};
    use coln_batch::generic_join;
    use coln_batch::query::Catalog;
    use coln_batch::relation::Relation;

    // Small plan-building helpers, mirroring how the logical tests in
    // lib.rs write their plans, just terser.

    fn src(name: &str) -> SourceId {
        SourceId::from(name)
    }

    /// One catalog for every rung: the base tables a plan may name. The
    /// first three are plain unsigned integer tables; `person` carries
    /// every kind of column the engine types.
    fn test_sources() -> SourceSchemas {
        let uint_tables = [
            ("edge", vec!["from", "to"]),
            ("r", vec!["a", "b"]),
            ("s", vec!["c", "d"]),
        ]
        .into_iter()
        .map(|(name, columns)| {
            let columns = columns
                .into_iter()
                .map(|column| PlanColumn::new(column, PlanType::Uint))
                .collect();
            (
                SourceId::from(name),
                TableSchema::new(EntityRef::from(name), columns, vec![]),
            )
        });
        let person = (
            SourceId::from("person"),
            TableSchema::new(
                EntityRef::from("person"),
                vec![
                    PlanColumn::new("id", PlanType::Uint),
                    PlanColumn::new("name", PlanType::String),
                    PlanColumn::new("age", PlanType::Iint),
                    PlanColumn::new("active", PlanType::Bool),
                ],
                vec![],
            ),
        );
        uint_tables.chain(std::iter::once(person)).collect()
    }

    fn lower_plan(stmts: Vec<Stmt>) -> Result<LoweredPlan> {
        lower(&QueryIr::new(stmts), &test_sources())
    }

    fn let_rel(name: &str, initializer: impl Into<Expr>) -> Stmt {
        Stmt::from(VarStmt {
            name: name.to_string(),
            initializer: Some(initializer.into()),
        })
    }

    fn var(name: &str) -> Expr {
        Expr::from(VarExpr::new(name))
    }

    fn lit(value: u64) -> Expr {
        Expr::from(LiteralExpr {
            value: Literal::Uint(value),
        })
    }

    fn literal(value: Literal) -> Expr {
        Expr::from(LiteralExpr { value })
    }

    fn out(name: &str) -> Stmt {
        Stmt::from(ExprStmt {
            expr: Expr::from(OutputExpr {
                relation: var(name),
                id: SinkId::from(name),
                kind: OutputKind::Channel,
            }),
        })
    }

    fn join_variable(name: &str, occurrences: &[(usize, &str)]) -> JoinVariable {
        JoinVariable {
            name: name.to_string(),
            occurrences: occurrences
                .iter()
                .map(|(relation, column)| (*relation, var(column)))
                .collect(),
        }
    }

    fn named_columns(pairs: &[(&str, &str)]) -> Vec<(String, Expr)> {
        pairs
            .iter()
            .map(|(name, column)| (name.to_string(), var(column)))
            .collect()
    }

    fn equals(column: &str, value: Literal) -> Expr {
        Expr::from(BinaryExpr {
            operator: Operator::Equal,
            left: var(column),
            right: literal(value),
        })
    }

    /// Run a lowered plan over hand-built relations with coln-batch's
    /// semi-naive fixpoint and the worst-case-optimal executor.
    fn run(plan: &LoweredPlan, edb: Catalog) -> Catalog {
        fixpoint::semi_naive(&plan.program, &edb, generic_join::execute as Exec)
            .unwrap()
            .catalog
    }

    fn rel(name: &str, columns: [&str; 2], rows_: &[(u64, u64)]) -> Relation {
        Relation::new(
            name,
            columns,
            vec![
                rows_.iter().map(|r| r.0).collect(),
                rows_.iter().map(|r| r.1).collect(),
            ],
        )
    }

    fn rows(relation: &Relation) -> Vec<Vec<u64>> {
        (0..relation.len()).map(|i| relation.row(i)).collect()
    }

    /// The `person` table with typed rows, in a catalog of its own.
    fn people() -> Catalog {
        let mut edb = Catalog::new();
        edb.insert_rows(
            "person",
            Schema::new([
                Column::new("id", ScalarType::Uint),
                Column::new("name", ScalarType::String),
                Column::new("age", ScalarType::Iint),
                Column::new("active", ScalarType::Bool),
            ]),
            vec![
                vec![1u64.into(), "ann".into(), (-5i64).into(), true.into()],
                vec![2u64.into(), "bob".into(), 30i64.into(), false.into()],
                vec![3u64.into(), "ann".into(), 7i64.into(), true.into()],
            ],
        )
        .unwrap();
        edb
    }

    /// Step 1: a source becomes a stored atom, a variable becomes a
    /// derived relation, and an output names what to read. The smallest
    /// possible translation: one rule that copies the source.
    #[test]
    fn s01_source_becomes_a_stored_atom() {
        let plan = lower_plan(vec![
            let_rel("edges", SourceExpr::new(src("edge"))),
            out("edges"),
        ])
        .unwrap();

        assert_eq!(plan.sources["edge"].names(), vec!["from", "to"]);
        assert_eq!(plan.outputs["edges"], "edges");
        assert_eq!(plan.program.rules.len(), 1);
        let rule = &plan.program.rules[0];
        assert_eq!(rule.head.relation, "edges");
        assert_eq!(rule.body.len(), 1);
        assert_eq!(rule.body[0].relation, "edge");

        let result = run(&plan, {
            let mut edb = Catalog::new();
            edb.insert(rel("edge", ["from", "to"], &[(0, 1), (1, 2)]));
            edb
        });
        assert_eq!(
            rows(result.get("edges").unwrap()),
            vec![vec![0, 1], vec![1, 2]]
        );
    }

    /// Step 2: a projection is just the rule head. Selecting, reordering
    /// and renaming columns costs nothing at runtime.
    #[test]
    fn s02_projection_is_the_rule_head() {
        let plan = lower_plan(vec![
            let_rel("edges", SourceExpr::new(src("edge"))),
            let_rel(
                "swapped",
                ProjectionExpr {
                    relation: var("edges"),
                    attributes: named_columns(&[("target", "to"), ("origin", "from")]),
                },
            ),
            out("swapped"),
        ])
        .unwrap();

        assert_eq!(plan.schemas["swapped"].names(), vec!["target", "origin"]);

        let result = run(&plan, {
            let mut edb = Catalog::new();
            edb.insert(rel("edge", ["from", "to"], &[(0, 1)]));
            edb
        });
        assert_eq!(rows(result.get("swapped").unwrap()), vec![vec![1, 0]]);
    }

    /// Step 3: a projection can also pin a literal column. It lands as a
    /// literal directly in the rule head.
    #[test]
    fn s03_projection_can_pin_a_literal() {
        let plan = lower_plan(vec![
            let_rel("edges", SourceExpr::new(src("edge"))),
            let_rel(
                "tagged",
                ProjectionExpr {
                    relation: var("edges"),
                    attributes: vec![
                        ("from".to_string(), var("from")),
                        ("tag".to_string(), lit(7)),
                    ],
                },
            ),
            out("tagged"),
        ])
        .unwrap();

        let head = &plan.program.rules[1].head;
        assert_eq!(head.terms[1], Term::lit(7u64));

        let result = run(&plan, {
            let mut edb = Catalog::new();
            edb.insert(rel("edge", ["from", "to"], &[(3, 4)]));
            edb
        });
        assert_eq!(rows(result.get("tagged").unwrap()), vec![vec![3, 7]]);
    }

    /// Step 4: distinct is free. coln-batch results are sets already, so
    /// the operator lowers to nothing at all.
    #[test]
    fn s04_distinct_is_free() {
        let plain = lower_plan(vec![
            let_rel("edges", SourceExpr::new(src("edge"))),
            out("edges"),
        ])
        .unwrap();
        let deduped = lower_plan(vec![
            let_rel(
                "edges",
                DistinctExpr {
                    relation: Expr::from(SourceExpr::new(src("edge"))),
                },
            ),
            out("edges"),
        ])
        .unwrap();

        assert_eq!(plain.program.rules.len(), deduped.program.rules.len());
        assert_eq!(
            plain.program.rules[0].body.len(),
            deduped.program.rules[0].body.len()
        );
    }

    /// Step 5: an equality selection pins a column to a literal inside
    /// the atom. The executor then only ever sees matching rows; there is
    /// no separate filter step.
    #[test]
    fn s05_selection_pins_a_column() {
        let plan = lower_plan(vec![
            let_rel(
                "from_three",
                SelectionExpr {
                    relation: Expr::from(SourceExpr::new(src("edge"))),
                    condition: equals("from", Literal::Uint(3)),
                },
            ),
            out("from_three"),
        ])
        .unwrap();

        let body_atom = &plan.program.rules[0].body[0];
        assert_eq!(body_atom.terms[0], Term::lit(3u64));

        let result = run(&plan, {
            let mut edb = Catalog::new();
            edb.insert(rel("edge", ["from", "to"], &[(3, 4), (5, 6)]));
            edb
        });
        assert_eq!(rows(result.get("from_three").unwrap()), vec![vec![3, 4]]);
    }

    /// Step 6: an equi join is a shared variable. Both operands become
    /// atoms in the same body, and every `on` pair merges one variable of
    /// each side into one. That is the whole translation of a join.
    #[test]
    fn s06_equi_join_is_a_shared_variable() {
        let plan = lower_plan(vec![
            let_rel(
                "joined",
                EquiJoinExpr {
                    left: Expr::from(SourceExpr::new(src("r"))),
                    right: Expr::from(SourceExpr::new(src("s"))),
                    on: vec![(var("b"), var("c"))],
                    attributes: None,
                },
            ),
            out("joined"),
        ])
        .unwrap();

        let rule = &plan.program.rules[0];
        assert_eq!(rule.body.len(), 2);
        // The join column is the same variable in both atoms.
        assert_eq!(rule.body[0].terms[1], rule.body[1].terms[0]);

        let result = run(&plan, {
            let mut edb = Catalog::new();
            edb.insert(rel("r", ["a", "b"], &[(1, 10), (2, 20)]));
            edb.insert(rel("s", ["c", "d"], &[(10, 100), (30, 300)]));
            edb
        });
        assert_eq!(
            rows(result.get("joined").unwrap()),
            vec![vec![1, 10, 10, 100]]
        );
    }

    /// Step 7: chained joins flatten into one n-ary body. The nested
    /// logical tree becomes a single rule with three atoms, and the
    /// executor picks its own join order. This is the agreed stopgap for
    /// the missing n-ary join node in the shared plan.
    #[test]
    fn s07_join_chains_flatten_into_one_body() {
        let plan = lower_plan(vec![
            let_rel("e", SourceExpr::new(src("edge"))),
            let_rel(
                "three_hops",
                EquiJoinExpr {
                    left: Expr::from(EquiJoinExpr {
                        left: Expr::from(AliasExpr {
                            relation: var("e"),
                            alias: "h1".to_string(),
                        }),
                        right: Expr::from(AliasExpr {
                            relation: var("e"),
                            alias: "h2".to_string(),
                        }),
                        on: vec![(var("to"), var("from"))],
                        attributes: Some(named_columns(&[("start", "h1.from"), ("mid", "h2.to")])),
                    }),
                    right: Expr::from(AliasExpr {
                        relation: var("e"),
                        alias: "h3".to_string(),
                    }),
                    on: vec![(var("mid"), var("from"))],
                    attributes: Some(named_columns(&[("start", "start"), ("end", "h3.to")])),
                },
            ),
            out("three_hops"),
        ])
        .unwrap();

        // One rule, three atoms: the tree flattened.
        let rule = &plan.program.rules[1];
        assert_eq!(rule.body.len(), 3);

        let result = run(&plan, {
            let mut edb = Catalog::new();
            edb.insert(rel("edge", ["from", "to"], &[(0, 1), (1, 2), (2, 3)]));
            edb
        });
        assert_eq!(rows(result.get("three_hops").unwrap()), vec![vec![0, 3]]);
    }

    /// Step 8: a cartesian product is the same translation with an empty
    /// `on` list: two atoms that simply share no variable.
    #[test]
    fn s08_cartesian_product_shares_no_variable() {
        let plan = lower_plan(vec![
            let_rel(
                "pairs",
                CartesianProductExpr::new(
                    Expr::from(SourceExpr::new(src("r"))),
                    Expr::from(SourceExpr::new(src("s"))),
                    None,
                ),
            ),
            out("pairs"),
        ])
        .unwrap();

        let result = run(&plan, {
            let mut edb = Catalog::new();
            edb.insert(rel("r", ["a", "b"], &[(1, 1), (2, 2)]));
            edb.insert(rel("s", ["c", "d"], &[(9, 9)]));
            edb
        });
        assert_eq!(result.get("pairs").unwrap().len(), 2);
    }

    /// Step 9: the n-ary join FLIR emits, consumed natively. One
    /// MultiWayEquiJoinExpr lowers to a single rule body with one
    /// variable per equality class, no flattening involved. The cyclic
    /// triangle is the shape that needs this: every pair of atoms
    /// shares a variable, and the executor must see all three atoms at
    /// once.
    #[test]
    fn s09_multi_way_join_is_native() {
        let triangle = MultiWayEquiJoinExpr::new(
            vec![var("e"), var("e"), var("e")],
            vec![
                join_variable("x", &[(0, "from"), (2, "to")]),
                join_variable("y", &[(0, "to"), (1, "from")]),
                join_variable("z", &[(1, "to"), (2, "from")]),
            ],
            None,
        )
        .unwrap();
        let plan = lower_plan(vec![
            let_rel("e", SourceExpr::new(src("edge"))),
            let_rel("triangles", triangle),
            out("triangles"),
        ])
        .unwrap();

        // One rule, three atoms over the derived edge relation, and the
        // documented schema fold: every join variable appears exactly
        // once, under its variable name.
        let rule = &plan.program.rules[1];
        assert_eq!(rule.body.len(), 3);
        assert_eq!(plan.schemas["triangles"].names(), vec!["x", "y", "z"]);

        // Edges 1→2→3→1 close a triangle; 1→4 is a dead end. The three
        // result rows are the rotations of the one triangle.
        let result = run(&plan, {
            let mut edb = Catalog::new();
            edb.insert(rel(
                "edge",
                ["from", "to"],
                &[(1, 2), (2, 3), (3, 1), (1, 4)],
            ));
            edb
        });
        assert_eq!(
            rows(result.get("triangles").unwrap()),
            vec![vec![1, 2, 3], vec![2, 3, 1], vec![3, 1, 2]]
        );
    }

    /// Step 10: aliases only affect name resolution. `h1.from` and a plain
    /// `from` resolve to the same variable; nothing changes in the rule.
    #[test]
    fn s10_aliases_are_pure_name_resolution() {
        let plan = lower_plan(vec![
            let_rel(
                "joined",
                EquiJoinExpr {
                    left: Expr::from(AliasExpr {
                        relation: Expr::from(SourceExpr::new(src("r"))),
                        alias: "left".to_string(),
                    }),
                    right: Expr::from(AliasExpr {
                        relation: Expr::from(SourceExpr::new(src("s"))),
                        alias: "right".to_string(),
                    }),
                    on: vec![(var("b"), var("c"))],
                    attributes: Some(named_columns(&[("a", "left.a"), ("d", "right.d")])),
                },
            ),
            out("joined"),
        ])
        .unwrap();

        assert_eq!(plan.schemas["joined"].names(), vec!["a", "d"]);

        let result = run(&plan, {
            let mut edb = Catalog::new();
            edb.insert(rel("r", ["a", "b"], &[(1, 10)]));
            edb.insert(rel("s", ["c", "d"], &[(10, 100)]));
            edb
        });
        assert_eq!(rows(result.get("joined").unwrap()), vec![vec![1, 100]]);
    }

    /// Step 11: referencing a bound variable places an atom over the
    /// derived relation, exactly like referencing a source. The chain of
    /// statements becomes a chain of rules, and the fixpoint evaluation
    /// orders them by data dependency on its own.
    #[test]
    fn s11_bound_variables_become_derived_atoms() {
        let plan = lower_plan(vec![
            let_rel("edges", SourceExpr::new(src("edge"))),
            let_rel(
                "targets",
                ProjectionExpr {
                    relation: var("edges"),
                    attributes: named_columns(&[("node", "to")]),
                },
            ),
            out("targets"),
        ])
        .unwrap();

        // The second rule's body reads the *derived* relation "edges".
        assert_eq!(plan.program.rules[1].body[0].relation, "edges");

        let result = run(&plan, {
            let mut edb = Catalog::new();
            edb.insert(rel("edge", ["from", "to"], &[(0, 1), (1, 2)]));
            edb
        });
        assert_eq!(rows(result.get("targets").unwrap()), vec![vec![1], vec![2]]);
    }

    /// Step 12: a union is one rule per branch with the same head. Set
    /// semantics deduplicate overlaps for free.
    #[test]
    fn s12_union_is_one_rule_per_branch() {
        let plan = lower_plan(vec![
            let_rel("left", SourceExpr::new(src("r"))),
            let_rel("right", SourceExpr::new(src("s"))),
            let_rel(
                "both",
                UnionExpr {
                    relations: vec![var("left"), var("right")],
                },
            ),
            out("both"),
        ])
        .unwrap();

        let heads: Vec<&str> = plan
            .program
            .rules
            .iter()
            .filter(|rule| rule.head.relation == "both")
            .map(|rule| rule.body[0].relation.as_str())
            .collect();
        assert_eq!(heads, vec!["left", "right"]);

        let result = run(&plan, {
            let mut edb = Catalog::new();
            edb.insert(rel("r", ["a", "b"], &[(1, 1), (2, 2)]));
            edb.insert(rel("s", ["c", "d"], &[(2, 2), (3, 3)]));
            edb
        });
        assert_eq!(result.get("both").unwrap().len(), 3);
    }

    /// Step 13: the payoff. A fixed point lowers to a base rule plus a
    /// recursive step rule, and coln-batch's semi-naive evaluation
    /// computes the closure. This is the arithmetic-free shape of Leo's
    /// `test_iteration`, translated end to end.
    #[test]
    fn s13_fixed_point_reaches_the_closure() {
        let plan = lower_plan(vec![
            let_rel("edges", SourceExpr::new(src("edge"))),
            let_rel(
                "closure",
                FixedPointIterExpr {
                    accumulator: ("cur".to_string(), var("edges")),
                    step: BlockStmt {
                        stmts: vec![Stmt::from(ExprStmt {
                            expr: Expr::from(EquiJoinExpr {
                                left: Expr::from(AliasExpr {
                                    relation: var("cur"),
                                    alias: "walk".to_string(),
                                }),
                                right: Expr::from(AliasExpr {
                                    relation: var("edges"),
                                    alias: "step".to_string(),
                                }),
                                on: vec![(var("to"), var("from"))],
                                attributes: Some(named_columns(&[
                                    ("from", "walk.from"),
                                    ("to", "step.to"),
                                ])),
                            }),
                        })],
                    },
                },
            ),
            out("closure"),
        ])
        .unwrap();

        // A base rule and a recursive rule, both defining "closure"; the
        // step rule reads "closure" in its own body.
        let closure_rules: Vec<_> = plan
            .program
            .rules
            .iter()
            .filter(|rule| rule.head.relation == "closure")
            .collect();
        assert_eq!(closure_rules.len(), 2);
        assert!(
            closure_rules[1]
                .body
                .iter()
                .any(|atom| atom.relation == "closure")
        );

        let result = run(&plan, {
            let mut edb = Catalog::new();
            edb.insert(rel("edge", ["from", "to"], &[(0, 1), (1, 2), (2, 3)]));
            edb
        });
        assert_eq!(
            rows(result.get("closure").unwrap()),
            vec![
                vec![0, 1],
                vec![0, 2],
                vec![0, 3],
                vec![1, 2],
                vec![1, 3],
                vec![2, 3],
            ]
        );
    }

    /// Step 14: everything outside this slice fails loudly with a clear
    /// message instead of producing a wrong answer.
    #[test]
    fn s14_unsupported_features_fail_loudly() {
        let err = lower_plan(vec![let_rel(
            "anti",
            Expr::from(AntiJoinExpr {
                left: Expr::from(SourceExpr::new(src("r"))),
                right: Expr::from(SourceExpr::new(src("s"))),
                on: vec![(var("a"), var("c"))],
            }),
        )])
        .unwrap_err();
        assert!(err.to_string().contains("AntiJoin"));

        let err = lower_plan(vec![let_rel(
            "computed",
            ProjectionExpr {
                relation: Expr::from(SourceExpr::new(src("r"))),
                attributes: vec![(
                    "sum".to_string(),
                    Expr::from(BinaryExpr {
                        operator: Operator::Less,
                        left: var("a"),
                        right: var("b"),
                    }),
                )],
            },
        )])
        .unwrap_err();
        assert!(format!("{err:#}").contains("scalar engine"), "{err:#}");

        let err = lower_plan(vec![let_rel("missing", SourceExpr::new(src("nope")))]).unwrap_err();
        assert!(err.to_string().contains("not in the catalog"));

        let err = lower_plan(vec![let_rel(
            "nested",
            ProjectionExpr {
                relation: Expr::from(OutputExpr {
                    relation: Expr::from(SourceExpr::new(src("r"))),
                    id: SinkId::from("inner"),
                    kind: OutputKind::Channel,
                }),
                attributes: named_columns(&[("a", "a")]),
            },
        )])
        .unwrap_err();
        assert!(err.to_string().contains("nested"));
    }

    /// Step 15: columns keep their types. A source's schema types the
    /// rule variables, a selection may pin a string or a signed integer,
    /// and a derived relation's schema carries the types on, so the
    /// engine decodes real values on the way out.
    #[test]
    fn s15_columns_keep_their_types() {
        let plan = lower_plan(vec![
            let_rel(
                "anns",
                SelectionExpr {
                    relation: Expr::from(SourceExpr::new(src("person"))),
                    condition: equals("name", Literal::String("ann".into())),
                },
            ),
            let_rel(
                "young",
                SelectionExpr {
                    relation: var("anns"),
                    condition: equals("age", Literal::Iint(-5)),
                },
            ),
            let_rel(
                "picked",
                ProjectionExpr {
                    relation: var("young"),
                    attributes: vec![
                        ("who".to_string(), var("name")),
                        ("id".to_string(), var("id")),
                        ("note".to_string(), literal(Literal::String("seen".into()))),
                    ],
                },
            ),
            out("picked"),
        ])
        .unwrap();

        assert_eq!(
            plan.sources["person"].types(),
            vec![
                ScalarType::Uint,
                ScalarType::String,
                ScalarType::Iint,
                ScalarType::Bool
            ]
        );
        assert_eq!(plan.schemas["picked"].names(), vec!["who", "id", "note"]);
        assert_eq!(
            plan.schemas["picked"].types(),
            vec![ScalarType::String, ScalarType::Uint, ScalarType::String]
        );

        let result = run(&plan, people());
        assert_eq!(
            result.rows_of("picked").unwrap(),
            vec![vec!["ann".into(), 1u64.into(), "seen".into()]]
        );
        // The unknown name never matches, and that is not an error.
        let plan = lower_plan(vec![
            let_rel(
                "nobody",
                SelectionExpr {
                    relation: Expr::from(SourceExpr::new(src("person"))),
                    condition: equals("name", Literal::String("zed".into())),
                },
            ),
            out("nobody"),
        ])
        .unwrap();
        assert!(run(&plan, people()).get("nobody").unwrap().is_empty());
    }

    /// Step 16: type errors are lowering errors. Joining or comparing
    /// columns of different types is rejected before any data is read.
    #[test]
    fn s16_type_mismatches_fail_at_lowering() {
        // A uint column joined with a string column.
        let err = lower_plan(vec![let_rel(
            "bad_join",
            EquiJoinExpr {
                left: Expr::from(SourceExpr::new(src("r"))),
                right: Expr::from(SourceExpr::new(src("person"))),
                on: vec![(var("a"), var("name"))],
                attributes: None,
            },
        )])
        .unwrap_err();
        assert!(
            err.to_string().contains("uint column with a string column"),
            "{err}"
        );

        // A string column compared with an integer literal.
        let err = lower_plan(vec![let_rel(
            "bad_selection",
            SelectionExpr {
                relation: Expr::from(SourceExpr::new(src("person"))),
                condition: equals("name", Literal::Uint(3)),
            },
        )])
        .unwrap_err();
        assert!(
            err.to_string().contains("(string) with the uint literal 3"),
            "{err}"
        );

        // A multi-way join variable spanning two types.
        let err = lower_plan(vec![let_rel(
            "bad_multi",
            MultiWayEquiJoinExpr::new(
                vec![
                    Expr::from(SourceExpr::new(src("r"))),
                    Expr::from(SourceExpr::new(src("person"))),
                ],
                vec![join_variable("k", &[(0, "a"), (1, "age")])],
                None,
            )
            .unwrap(),
        )])
        .unwrap_err();
        assert!(err.to_string().contains("join variable k"), "{err}");

        // Union branches must agree on types, not only on arity.
        let err = lower_plan(vec![
            let_rel(
                "ids",
                ProjectionExpr {
                    relation: Expr::from(SourceExpr::new(src("person"))),
                    attributes: named_columns(&[("v", "id")]),
                },
            ),
            let_rel(
                "names",
                ProjectionExpr {
                    relation: Expr::from(SourceExpr::new(src("person"))),
                    attributes: named_columns(&[("v", "name")]),
                },
            ),
            let_rel(
                "mixed",
                UnionExpr {
                    relations: vec![var("ids"), var("names")],
                },
            ),
        ])
        .unwrap_err();
        assert!(err.to_string().contains("different schemas"), "{err}");
    }
}
