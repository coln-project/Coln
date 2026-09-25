// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module does static analysis of a Datalog program.

use std::fmt;

use super::graph_utils::gabow;
use super::{AggregateRules, Component, Identifiable, LogicalProgram, Predicate, Rule};
use crate::frontend::Atom;
use crate::frontend::graph_utils::vertex_degrees;
use anyhow::bail;
use indexmap::IndexMap;

pub(super) fn static_analysis_pipeline<'a, LP: LogicalProgram>(
    logical_program: &'a LP,
) -> Result<ExecutionOrder<'a, LP::Predicate>, anyhow::Error> {
    let predicate_dep_graph = PredicateDependencyGraph::from_logical_program(logical_program);
    let quotient_graph =
        QuotientGraph::<LP::Predicate>::from_predicate_dep_graph(&predicate_dep_graph);
    if !quotient_graph.is_stratifiable() {
        bail!("Not stratifiable") // TODO: Nice error reporting.
    }
    Ok(quotient_graph.into_execution_order())
}

pub(super) type ExecutionOrder<'a, P> = Vec<PredicateComponent<'a, P>>;
type NodeIdx = usize;
type Neighbors<EdgeData> = Vec<AdjacentNode<EdgeData>>;
type Adjacency<EdgeData> = Vec<Neighbors<EdgeData>>;

#[derive(Debug)]
struct AdjacentNode<Data> {
    /// To which node the adjacent node is pointing.
    node: NodeIdx,
    data: Data,
}

#[derive(Debug, Clone, Copy)]
enum EdgeLabel {
    /// An edge is positive if its dependency `predA -> predB` is not negated.
    Positive,
    /// An edge is negative if its dependency `predA -> predB` is negated.
    Negative,
}

/// A strongly connected component (SCC) and contains predicates referencing
/// each other through some cycle.
#[derive(Debug)]
pub(super) struct PredicateComponent<'a, P> {
    /// All predicates within the SCC.
    members: Vec<&'a P>,
    /// Edges within a component.
    intra_adjacency: Adjacency<EdgeLabel>,
}

impl<P: Predicate> AggregateRules for PredicateComponent<'_, P> {
    type Identifier = P::Identifier;
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
struct PredicateDependencyGraph<'a, LP: LogicalProgram> {
    predicates: IndexMap<&'a LP::Identifier, &'a LP::Predicate>,
    adjacency: Adjacency<EdgeLabel>,
}

impl<'a, LP: LogicalProgram> PredicateDependencyGraph<'a, LP> {
    fn from_logical_program(program: &'a LP) -> PredicateDependencyGraph<'a, LP> {
        // A predicate's position in `predicates` is its node id in the graphs
        // computed later, and the map's keys are what atoms are tested against
        // to compute the dependencies. This mapping is local to this function,
        // so the node ids are stable only within the context of this function.
        let predicates: IndexMap<&'a LP::Identifier, &'a LP::Predicate> = program
            .predicates()
            .map(|predicate| (predicate.id(), predicate))
            // TODO: This silently overwrites.
            .collect();
        // TODO: What if an atom is dangling?
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
            adjacency,
        }
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

impl<'a, P> QuotientGraph<'a, P> {
    fn from_predicate_dep_graph<LP: LogicalProgram>(
        graph: &PredicateDependencyGraph<'a, LP>,
    ) -> QuotientGraph<'a, LP::Predicate> {
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
            Vec<PredicateComponent<LP::Predicate>>,
        ) = components
            .iter()
            .map(|component| {
                let inter_edges_neighbors: Neighbors<EdgeLabel> = Vec::new();
                let intra_edges_adjacency: Adjacency<EdgeLabel> =
                    (0..components.len()).map(|_| Vec::new()).collect();
                let members = component
                    .iter()
                    .map(|member| {
                        *graph
                            .predicates
                            .get_index(*member)
                            .expect("valid node idx")
                            .1
                    })
                    .collect::<Vec<&LP::Predicate>>();
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
                    if matches!(EdgeLabel::Negative, label) {
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

    fn into_execution_order(self) -> ExecutionOrder<'a, P> {
        self.components
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
