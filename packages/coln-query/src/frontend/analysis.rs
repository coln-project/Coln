// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module does static analysis of a Datalog program.

use super::display::separated;
use super::graph_utils::gabow;
use super::{AggregateRules, Component, IdOf, Identifiable, LogicalProgram, Predicate, Rule};
use crate::error::SyntaxError;
use crate::frontend::Atom;
use crate::frontend::graph_utils::vertex_degrees;
use crate::relational::schema::Column;
use indexmap::{IndexMap, IndexSet, map::Entry};
use std::{collections::HashSet, fmt, marker::PhantomData};

/// Builds the program's graphs, checking nothing. See [`Analysis::verify`].
pub(super) fn analyze<LP: LogicalProgram>(program: &LP) -> Analysis<'_, LP::Predicate> {
    let dependencies = PredicateDependencyGraph::from_logical_program(program);
    let quotient = QuotientGraph::from_predicate_dep_graph(&dependencies);
    Analysis {
        dependencies,
        quotient,
        state: PhantomData,
    }
}

/// The program's components in the order to evaluate them in, each after
/// every component it depends on.
pub(super) type ExecutionOrder<'a, P> = [PredicateComponent<'a, P>];

/// An [`Analysis`] state: the graphs are built, but the program may be invalid.
pub struct Unchecked;

/// An [`Analysis`] state: the program passed every check, so it may be
/// translated.
pub struct Verified;

/// What static analysis finds out about a program: its predicate dependency
/// graph, and the quotient graph of components laid over it.
///
/// [`LogicalProgram::analyze`] builds one in the [`Unchecked`] state, which
/// [`verify`](Analysis::verify) turns into the [`Verified`] state if the
/// program is valid. Only a verified analysis offers an
/// [`execution_order`](Analysis::execution_order), and only it is accepted by
/// [`LogicalProgram::prepare`]: a program cannot be translated unchecked.
///
/// In either state, the graphs can be read through
/// [`components`](Self::components) and [`edges`](Self::edges), which speak
/// in predicates rather than in the graphs' node indices, and drawn with
/// [`dot`](Self::dot). That includes the graphs of an invalid program, which
/// [`Rejected`] hands back.
pub struct Analysis<'a, P: Identifiable, S = Unchecked> {
    dependencies: PredicateDependencyGraph<'a, P>,
    quotient: QuotientGraph<'a, P>,
    state: PhantomData<S>,
}

impl<'a, P: Predicate> Analysis<'a, P, Unchecked> {
    /// Checks the program: it has to be well-formed (see [`Malformation`]),
    /// and stratifiable. On failure, the analysis comes back along with the
    /// error, so that it can still be inspected.
    ///
    /// Well-formedness is checked first, since a malformed program's graphs
    /// miss what is malformed: a dangling negated atom adds no edge, so it
    /// cannot break a stratification either.
    pub fn verify(self) -> Result<Analysis<'a, P, Verified>, Rejected<'a, P>> {
        let malformations = self.dependencies.malformations();
        if !malformations.is_empty() {
            let message = fmt::from_fn(|f| separated(f, &malformations, "; ")).to_string();
            return Err(Rejected {
                error: SyntaxError::new(message),
                analysis: Box::new(self),
            });
        }
        if !self.quotient.is_stratifiable() {
            // TODO: Nice error reporting.
            return Err(Rejected {
                error: SyntaxError::new("not stratifiable"),
                analysis: Box::new(self),
            });
        }
        Ok(Analysis {
            dependencies: self.dependencies,
            quotient: self.quotient,
            state: PhantomData,
        })
    }
}

impl<'a, P: Predicate> Analysis<'a, P, Verified> {
    pub(super) fn execution_order(&self) -> &ExecutionOrder<'a, P> {
        &self.quotient.components
    }
}

/// An [`Analysis`] whose program failed [verification](Analysis::verify),
/// along with why.
///
/// Deliberately not convertible into a [`SyntaxError`] by `?`: an impl of
/// `From` for it makes `?` ambiguous wherever the error type is inferred.
/// Take [`error`](Self::error) instead.
pub struct Rejected<'a, P: Identifiable> {
    pub error: SyntaxError,
    /// Boxed to keep the error path of a verification small.
    pub analysis: Box<Analysis<'a, P>>,
}

/// Shows the error only, so that a rejection can be `unwrap`ped.
impl<P: Identifiable> fmt::Debug for Rejected<'_, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rejected")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

/// Shows nothing of the graphs, which [`dot`](Analysis::dot) draws instead,
/// so that a verification can be `unwrap`ped either way.
impl<P: Identifiable, S> fmt::Debug for Analysis<'_, P, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Analysis").finish_non_exhaustive()
    }
}

impl<'a, P: Predicate, S> Analysis<'a, P, S> {
    /// The components, each after every component it depends on, which is the
    /// order a [`Verified`] analysis executes them in.
    pub(super) fn components(&self) -> impl Iterator<Item = ComponentView<'_, 'a, P>> {
        let quotient = &self.quotient;
        quotient
            .components
            .iter()
            .enumerate()
            .map(|(position, component)| ComponentView {
                position,
                component,
                is_source: quotient.sources.contains(&position),
                is_sink: quotient.sinks.contains(&position),
            })
    }

    /// Every dependency between two predicates, once per polarity: a
    /// predicate reading another in several atoms of the same polarity
    /// depends on it once.
    pub(super) fn edges(&self) -> impl Iterator<Item = DependencyEdge<'a, P>> {
        let dependencies = &self.dependencies;
        let component_of = &self.quotient.component_of;
        let distinct: IndexSet<(NodeIdx, NodeIdx, EdgeLabel)> = dependencies
            .adjacency
            .iter()
            .enumerate()
            .flat_map(|(from, neighbors)| {
                neighbors
                    .iter()
                    .map(move |neighbor| (from, neighbor.node, neighbor.data))
            })
            .collect();
        distinct
            .into_iter()
            .map(move |(from, to, label)| DependencyEdge {
                from: dependencies.predicate(from),
                to: dependencies.predicate(to),
                negative: matches!(label, EdgeLabel::Negative),
                intra: component_of[from] == component_of[to],
            })
    }
}

/// A component of an [`Analysis`], as its graphs place it.
pub(super) struct ComponentView<'g, 'a, P> {
    /// Its position in the order of [`components`](Analysis::components).
    pub(super) position: usize,
    pub(super) component: &'g PredicateComponent<'a, P>,
    /// Depends on no other component, that is, an input of the dataflow.
    pub(super) is_source: bool,
    /// No other component depends on it, that is, an output of the dataflow.
    pub(super) is_sink: bool,
}

/// A dependency of predicate `from` on predicate `to`, which `from` mentions
/// in one of its body's atoms.
pub(super) struct DependencyEdge<'a, P> {
    pub(super) from: &'a P,
    pub(super) to: &'a P,
    /// Mentioned in a negated atom.
    pub(super) negative: bool,
    /// Both ends belong to the same component.
    pub(super) intra: bool,
}
type NodeIdx = usize;
type Neighbors<EdgeData> = Vec<AdjacentNode<EdgeData>>;
type Adjacency<EdgeData> = Vec<Neighbors<EdgeData>>;

#[derive(Debug, Clone)]
struct AdjacentNode<Data> {
    /// To which node the adjacent node is pointing.
    node: NodeIdx,
    data: Data,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum EdgeLabel {
    /// An edge is positive if its dependency `predA -> predB` is not negated.
    Positive,
    /// An edge is negative if its dependency `predA -> predB` is negated.
    Negative,
}

/// A strongly connected component (SCC) and contains predicates referencing
/// each other through some cycle.
#[derive(Debug, Clone)]
pub struct PredicateComponent<'a, P> {
    /// All predicates within the SCC.
    members: Vec<&'a P>,
    /// Edges within a component.
    intra_adjacency: Adjacency<EdgeLabel>,
}

impl<P: Predicate> AggregateRules for PredicateComponent<'_, P> {
    type Rule = P::Rule;

    fn rules(&self) -> impl Iterator<Item = &Self::Rule> {
        self.members
            .iter()
            .copied()
            .flat_map(|member| member.rules())
    }
}

impl<P: Predicate> Component for PredicateComponent<'_, P> {
    type Predicate = P;

    fn members(&self) -> impl Iterator<Item = &Self::Predicate> {
        self.members.iter().copied()
    }
}

/// A graph in which every vertex is a [LogicalProgram::Predicate] and each
/// edge `p1 -> p2`, whenever `p1` depends on `p2`, that is, `p1` mentions `p2`
/// in one of its body's [atoms](Atom).
struct PredicateDependencyGraph<'a, P: Identifiable> {
    predicates: IndexMap<&'a IdOf<P>, &'a P>,
    /// Predicates declared under a name an earlier predicate already took.
    /// They are no vertices, so that every name resolves to one predicate.
    duplicates: Vec<&'a P>,
    adjacency: Adjacency<EdgeLabel>,
}

impl<'a, P: Predicate> PredicateDependencyGraph<'a, P> {
    /// Builds the graph, leaving out what [`malformations`] reports: a
    /// duplicate declaration is no vertex, and an atom referencing no
    /// predicate is no edge.
    ///
    /// [`malformations`]: Self::malformations
    fn from_logical_program<LP: LogicalProgram<Predicate = P>>(program: &'a LP) -> Self {
        // A predicate's position in `predicates` is its node id in the graphs
        // computed later, and the map's keys are what atoms are tested against
        // to compute the dependencies.
        // The first declaration of a name wins, the ones after it are kept
        // aside for verification to report.
        let mut predicates: IndexMap<&'a IdOf<P>, &'a P> = IndexMap::new();
        let mut duplicates: Vec<&'a P> = Vec::new();
        for predicate in program.predicates() {
            match predicates.entry(predicate.id()) {
                Entry::Vacant(slot) => {
                    slot.insert(predicate);
                }
                Entry::Occupied(_) => duplicates.push(predicate),
            }
        }
        let adjacency: Adjacency<EdgeLabel> = predicates
            .values()
            .map(|predicate| {
                predicate
                    .rules()
                    .flat_map(|rule| rule.atoms())
                    .filter_map(|atom| {
                        predicates.get_index_of(atom.id()).map(|node_idx| {
                            let label = if atom.is_positive() {
                                EdgeLabel::Positive
                            } else {
                                EdgeLabel::Negative
                            };
                            AdjacentNode {
                                node: node_idx,
                                data: label,
                            }
                        })
                    })
                    .collect()
            })
            .collect();
        Self {
            predicates,
            duplicates,
            adjacency,
        }
    }

    /// Everything that makes the program ill-formed, regardless of what its
    /// graphs look like: duplicate declarations, atoms referencing no
    /// predicate, and atoms whose bindings do not fit the arity of the
    /// predicate they reference. See [`Malformation`].
    ///
    /// A head is checked against the predicate whose rule it heads.
    fn malformations(&self) -> Vec<Malformation<'a, P>> {
        let duplicates = self
            .duplicates
            .iter()
            .copied()
            .map(|predicate| Malformation::DuplicateDeclaration { predicate });
        let rules = self.predicates.values().copied().flat_map(|predicate| {
            predicate.rules().flat_map(move |rule| {
                let site = Site { predicate, rule };
                site.head_malformations()
                    .chain(self.body_malformations(site))
            })
        });
        duplicates.chain(rules).collect()
    }

    /// What is wrong with the body atoms of `site`'s rule: each has to
    /// reference a predicate, and fit that predicate's arity.
    fn body_malformations(&self, site: Site<'a, P>) -> impl Iterator<Item = Malformation<'a, P>> {
        site.rule.atoms().flat_map(move |atom| {
            let referenced = self.predicates.get(atom.id()).copied();
            let dangling = referenced
                .is_none()
                .then_some(Malformation::Dangling { site, atom });
            let misplaced = referenced
                .into_iter()
                .flat_map(move |referenced| misplaced_bindings(site, atom, referenced));
            dangling.into_iter().chain(misplaced)
        })
    }

    /// The predicate at node `idx`.
    fn predicate(&self, idx: NodeIdx) -> &'a P {
        self.predicates
            .get_index(idx)
            .map(|(_, predicate)| *predicate)
            .expect("valid node idx")
    }
}

/// The QuotientGraph is guaranteed to be a directed acyclic graph (DAG).
/// Each vertex is a [PredicateComponent] and each edge `c1 -> c2`, whenever
/// `c1` depends on `c2`, that is, one of the predicates within `c1` depend on
/// a predicate within `c2`.
struct QuotientGraph<'a, P> {
    /// The nodes of the quotient graph are the components and stored in
    /// reverse topological order.
    components: Vec<PredicateComponent<'a, P>>,
    /// Look up to which [component](`Self::components`) a node belongs to.
    component_of: Vec<NodeIdx>,
    /// Edges which connect components (cross edges).
    inter_adjacency: Adjacency<EdgeLabel>,
    /// Which components are sources/inputs (in terms of the dataflow but not
    /// in graph terms (sources would be the leafs of the graph, that is, all
    /// vertices with out-degree 0)).
    sources: Vec<NodeIdx>,
    /// Which components are sinks/outputs (in terms of the dataflow but not
    /// in graph terms (sinks would be the roots of the graph, that is, all
    /// vertices with in-degree 0)).
    sinks: Vec<NodeIdx>,
}

impl<'a, P: Predicate> QuotientGraph<'a, P> {
    fn from_predicate_dep_graph(graph: &PredicateDependencyGraph<'a, P>) -> Self {
        let gabow::Sccs {
            components,
            component_of,
        } = gabow::sccs(
            &graph
                .adjacency
                .iter()
                .map(|adjacency| adjacency.iter().map(|adjacency| adjacency.node).collect())
                .collect::<Vec<Vec<NodeIdx>>>(),
        );
        let (mut inter_adjacency, mut predicate_components): (
            Adjacency<EdgeLabel>,
            Vec<PredicateComponent<P>>,
        ) = components
            .iter()
            .map(|component| {
                let inter_edges_neighbors: Neighbors<EdgeLabel> = Vec::new();
                let intra_edges_adjacency: Adjacency<EdgeLabel> =
                    (0..component.len()).map(|_| Vec::new()).collect();
                let members = component
                    .iter()
                    .map(|member| graph.predicate(*member))
                    .collect::<Vec<&P>>();
                let predicate_component = PredicateComponent {
                    members,
                    intra_adjacency: intra_edges_adjacency,
                };
                (inter_edges_neighbors, predicate_component)
            })
            .collect();
        for (from, neighbors) in graph.adjacency.iter().enumerate() {
            let from_component = component_of[from];
            for neighbor in neighbors {
                let to = neighbor.node;
                let to_component = component_of[to];
                let label = neighbor.data;
                if from_component == to_component {
                    // We have an intra component edge.
                    let component = &components[from_component];
                    let predicate_component = &mut predicate_components[from_component];
                    // Find out the index of `node_idx` in terms of the nodes within a component.
                    let index_of = |node_idx: NodeIdx| {
                        component
                            .iter()
                            .position(|component_node_idx| *component_node_idx == node_idx)
                            .expect("valid node idx")
                    };
                    predicate_component.intra_adjacency[index_of(from)].push(AdjacentNode {
                        node: index_of(to),
                        data: label,
                    });
                } else {
                    // We have an inter component (cross) edge.
                    inter_adjacency[from_component].push(AdjacentNode {
                        node: to_component,
                        data: label,
                    });
                }
            }
        }
        let vertex_degrees = vertex_degrees(
            &inter_adjacency
                .iter()
                .map(|neighbors| neighbors.iter().map(|neighbor| neighbor.node).collect())
                .collect::<Vec<Vec<NodeIdx>>>(),
        );
        debug_assert!(
            vertex_degrees[0].out_degree == 0,
            "first SCC must be a source by construction"
        );
        debug_assert!(
            vertex_degrees[vertex_degrees.len() - 1].in_degree == 0,
            "last SCC must be a sink by construction"
        );
        QuotientGraph {
            components: predicate_components,
            component_of,
            inter_adjacency,
            sources: vertex_degrees
                .iter()
                .enumerate()
                .filter_map(|(component_idx, vertex_degree)| {
                    if vertex_degree.out_degree == 0 {
                        Some(component_idx)
                    } else {
                        None
                    }
                })
                .collect(),
            sinks: vertex_degrees
                .iter()
                .enumerate()
                .filter_map(|(component_idx, vertex_degree)| {
                    if vertex_degree.in_degree == 0 {
                        Some(component_idx)
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }

    /// Whether this program is valid under the restriction of stratified
    /// negation. Stratified negation entails:
    ///
    /// 1. The program's fixed points exist.
    /// 2. The program's fixed points are exactly one element (unique).
    ///
    /// Hence, the fixed point is stable, that is, you converge to it no matter
    /// in which order you execute it (given by (2)). Convergence (termination)
    /// is also guaranteed (given by (1)).
    ///
    /// A program is stratifiable iff each cycle in the [PredicateDependencyGraph]
    /// does not contain an edge with a negative label. Equivalently, each
    /// predicate vertex of an SCC (component) may not have an edge with a
    /// negative label to another predicate vertex which is part of the same SCC.
    fn is_stratifiable(&self) -> bool {
        for component in self.components.iter() {
            for adjacency in component.intra_adjacency.iter() {
                for AdjacentNode {
                    node: _,
                    data: label,
                } in adjacency
                {
                    if matches!(label, EdgeLabel::Negative) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Whether this program is valid under the restriction of _parity_
    /// stratified negation. Parity stratified negation entails:
    ///
    /// 1. The program's fixed points exist.
    /// 2. The program's fixed points are **not** unique.
    ///
    /// Hence, to which fixed point you converge depends on the execution order
    /// (indicated by (2)). Yet, convergence (termination) is still guaranteed
    /// because the program remains monotone (given by (1)).
    ///
    /// Note that parity stratified negation is a relaxation of stratified
    /// negation which buys more expressive power in exchange for the unstable
    /// (but still guaranteed) convergence.
    fn is_parity_stratifiable() -> bool {
        unimplemented!("Implement parity stratification someday for fun (and maybe profit)")
    }

    fn dead_code_analysis() {
        todo!("Some day do dead code analysis")
    }
}

// TODO: Report negative cycles properly.
/// A negated atom reading a predicate of its own component, as reported by
/// [`QuotientGraph::is_stratifiable`]. Borrows the offending site from the
/// program so that a diagnostic can point at it.
///
/// Such a program has no stratification. Negation needs the relation it reads
/// to be complete before it is read, and a component derives all of its members at
/// once, so the atom asks for a final answer from a relation that is still
/// growing. `p :- !p` is the smallest case; mutual recursion through a negation
/// spreads the same contradiction over several predicates.
#[derive(Debug)]
struct NegativeCycle<'a, P: Predicate> {
    /// The predicate whose definition contains the offending atom.
    predicate: &'a P,
    /// The rule the negated atom sits in.
    rule: &'a P::Rule,
    /// The negated name, a member of `predicate`'s own component.
    identifier: &'a P::Identifier,
}

impl<P: Predicate> fmt::Display for NegativeCycle<'_, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "rule '{}' of predicate '{}' negates '{}', which the same component \
             derives and which is hence never complete when the negation \
             reads it",
            self.rule.id(),
            self.predicate.id(),
            self.identifier,
        )
    }
}

/// A rule, along with the predicate it belongs to, for a diagnostic to point at.
struct Site<'a, P: Predicate> {
    predicate: &'a P,
    rule: &'a P::Rule,
}

// Derived, they would demand `P: Clone` and `P: Copy`.
impl<P: Predicate> Clone for Site<'_, P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P: Predicate> Copy for Site<'_, P> {}

impl<'a, P: Predicate> Site<'a, P> {
    /// What is wrong with the rule's head: it has to fit the arity of the
    /// predicate it belongs to, and fill each of its columns.
    fn head_malformations(self) -> impl Iterator<Item = Malformation<'a, P>> {
        let head = self.rule.head();
        let filled: HashSet<usize> = head.bindings().map(|(position, _)| position).collect();
        let unfilled = self
            .predicate
            .columns()
            .enumerate()
            .filter(move |(position, _)| !filled.contains(position))
            .map(move |(position, column)| Malformation::ColumnUnfilled {
                site: self,
                position,
                column,
            });
        misplaced_bindings(self, head, self.predicate).chain(unfilled)
    }
}

/// The bindings of `atom`, which `site`'s rule contains, that do not fit
/// `referenced`, the predicate it references: positions beyond its arity, and
/// positions bound before.
fn misplaced_bindings<'a, P: Predicate>(
    site: Site<'a, P>,
    atom: &'a AtomOf<P>,
    referenced: &'a P,
) -> impl Iterator<Item = Malformation<'a, P>> {
    let arity = referenced.arity();
    let mut bound: HashSet<usize> = HashSet::new();
    atom.bindings().filter_map(move |(position, _)| {
        if position >= arity {
            Some(Malformation::PositionOutOfRange {
                site,
                atom,
                position,
                arity,
            })
        } else if !bound.insert(position) {
            Some(Malformation::PositionRepeated {
                site,
                atom,
                position,
            })
        } else {
            None
        }
    })
}

type AtomOf<P> = <<P as AggregateRules>::Rule as Rule>::Atom;

/// What makes a program ill-formed, as reported by
/// [`PredicateDependencyGraph::malformations`]. Borrows the offending site
/// from the program so that a diagnostic can point at it.
///
/// Unlike a [`NegativeCycle`], a malformation is visible without the graphs,
/// but it leaves them incomplete: a duplicate declaration is no vertex, and a
/// dangling atom no edge.
enum Malformation<'a, P: Predicate> {
    /// A predicate declared under a name an earlier predicate already took.
    DuplicateDeclaration { predicate: &'a P },
    /// A body atom referencing a name no predicate is declared under.
    Dangling {
        site: Site<'a, P>,
        atom: &'a AtomOf<P>,
    },
    /// An atom binding a position its predicate has no column at.
    PositionOutOfRange {
        site: Site<'a, P>,
        atom: &'a AtomOf<P>,
        position: usize,
        arity: usize,
    },
    /// An atom binding a position more than once.
    PositionRepeated {
        site: Site<'a, P>,
        atom: &'a AtomOf<P>,
        position: usize,
    },
    /// A head leaving a column of its predicate unfilled, with nothing to
    /// project into it.
    ColumnUnfilled {
        site: Site<'a, P>,
        position: usize,
        column: &'a Column,
    },
}

impl<P: Predicate> fmt::Display for Malformation<'_, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let in_rule = |f: &mut fmt::Formatter<'_>, site: &Site<'_, P>| {
            write!(
                f,
                "rule '{}' of predicate '{}' ",
                site.rule.id(),
                site.predicate.id()
            )
        };
        match self {
            Malformation::DuplicateDeclaration { predicate } => {
                write!(
                    f,
                    "predicate '{}' is declared more than once",
                    predicate.id()
                )
            }
            Malformation::Dangling { site, atom } => {
                in_rule(f, site)?;
                write!(f, "references '{}', which names no predicate", atom.id())
            }
            Malformation::PositionOutOfRange {
                site,
                atom,
                position,
                arity,
            } => {
                in_rule(f, site)?;
                write!(
                    f,
                    "binds position {position} of '{}', which has {arity} column(s)",
                    atom.id()
                )
            }
            Malformation::PositionRepeated {
                site,
                atom,
                position,
            } => {
                in_rule(f, site)?;
                write!(
                    f,
                    "binds position {position} of '{}' more than once",
                    atom.id()
                )
            }
            Malformation::ColumnUnfilled {
                site,
                position,
                column,
            } => {
                in_rule(f, site)?;
                write!(
                    f,
                    "leaves column {position} ('{}') of its head unfilled",
                    column.name()
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{LogicalProgram, RulePredicate, test_utils::*};
    use super::*;

    #[test]
    fn a_component_may_have_more_members_than_the_program_has_components() {
        // One component of three members, and no EDB relation that would add
        // components of its own.
        let program = program(vec![
            pred("p", &[&["q"]]),
            pred("q", &[&["r"]]),
            pred("r", &[&["p"]]),
        ]);
        let analysis = analyze(&program)
            .verify()
            .expect("a positive cycle is stratifiable");
        assert_eq!(analysis.execution_order().len(), 1);
    }

    #[test]
    fn a_head_has_to_fill_exactly_its_predicate_s_columns() {
        let predicates = RulePredicate::group(
            [declare("edge", &["x", "y"]), declare("path", &["x", "y"])],
            [
                rule("e0", "edge(x, y)", &[]),
                rule("r0", "path(x)", &["edge(x, y)"]),
                rule("r1", "path(x, y, z)", &["edge(x, y)", "edge(y, z)"]),
                rule("r2", "path(0: x, 0: y)", &["edge(x, y)"]),
            ],
        )
        .expect("every definand names a declared predicate");
        let program = program(predicates);
        let rejected = program
            .verify()
            .expect_err("every rule of `path` has a malformed head");
        assert_eq!(
            rejected.error.message(),
            "rule 'r0' of predicate 'path' leaves column 1 ('y') of its head unfilled; \
             rule 'r1' of predicate 'path' binds position 2 of 'path', which has 2 column(s); \
             rule 'r2' of predicate 'path' binds position 0 of 'path' more than once; \
             rule 'r2' of predicate 'path' leaves column 1 ('y') of its head unfilled"
        );
    }

    #[test]
    fn a_body_atom_has_to_fit_the_predicate_it_references() {
        // `r2`'s dangling atom is negated, which the graphs alone would miss:
        // it adds no edge, so it breaks no stratification either.
        let predicates = RulePredicate::group(
            [declare("edge", &["x", "y"]), declare("path", &["x", "y"])],
            [
                rule("e0", "edge(x, y)", &[]),
                rule("r0", "path(x, y)", &["edge(x, y, z)"]),
                rule("r1", "path(x, y)", &["edge(0: x, 0: y)"]),
                rule("r2", "path(x, y)", &["edge(x, y)", "!blocked(x, y)"]),
            ],
        )
        .expect("every definand names a declared predicate");
        let program = program(predicates);
        let rejected = program
            .verify()
            .expect_err("every rule of `path` has a malformed body atom");
        assert_eq!(
            rejected.error.message(),
            "rule 'r0' of predicate 'path' binds position 2 of 'edge', which has 2 column(s); \
             rule 'r1' of predicate 'path' binds position 0 of 'edge' more than once; \
             rule 'r2' of predicate 'path' references 'blocked', which names no predicate"
        );
    }

    #[test]
    fn a_predicate_may_be_declared_only_once() {
        let program = program(vec![pred("p", &[&[]]), pred("p", &[&[]])]);
        let rejected = program.verify().expect_err("`p` is declared twice");
        assert_eq!(
            rejected.error.message(),
            "predicate 'p' is declared more than once"
        );
    }

    #[test]
    fn a_rejected_program_hands_its_analysis_back() {
        // `p` negates itself, so `p` has no stratification.
        let program = program(vec![pred("q", &[&[]]), pred("p", &[&["q", "!p"]])]);
        let rejected = analyze(&program)
            .verify()
            .expect_err("a negative cycle is not stratifiable");
        assert_eq!(rejected.error.message(), "not stratifiable");
        assert!(
            rejected
                .analysis
                .edges()
                .any(|edge| edge.negative && edge.intra),
            "the analysis still shows the negative cycle"
        );
    }
}
