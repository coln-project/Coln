// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module does static analysis of a Datalog program.

use super::display::separated;
use super::graph_utils::gabow;
use super::{Cond, Identifiable, Language, LogicalProgram, Predicate, Rule, TypedVar};
use crate::error::SyntaxError;
use crate::frontend::Atom;
#[cfg(debug_assertions)]
use crate::frontend::graph_utils::VertexDegree;
use crate::frontend::graph_utils::vertex_degrees;
use crate::relational::schema::Column;
use indexmap::{IndexMap, IndexSet, map::Entry};
use std::{collections::HashSet, fmt, iter, marker::PhantomData};

/// Builds the program's graphs, checking nothing. See [`Analysis::verify`].
pub(super) fn analyze<LP: LogicalProgram>(program: &LP) -> Analysis<'_, LP::Lang> {
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
pub(super) type ExecutionOrder<'a, L> = [PredicateComponent<'a, L>];

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
pub struct Analysis<'a, L: Language, S = Unchecked> {
    dependencies: PredicateDependencyGraph<'a, L>,
    quotient: QuotientGraph<'a, L>,
    state: PhantomData<S>,
}

impl<'a, L: Language> Analysis<'a, L, Unchecked> {
    /// Checks the program: it has to be well-formed (see [`Malformation`]),
    /// stratifiable, and each of its components seeded (see [`Unseeded`]). On
    /// failure, the analysis comes back along with the error, so that it can
    /// still be inspected.
    ///
    /// Well-formedness is checked first, since a malformed program's graphs
    /// miss what is malformed: a dangling negated atom adds no edge, so it
    /// cannot break a stratification either. Likewise, an atom that reads no
    /// predicate cannot tell a base rule from a recursive one.
    pub fn verify(self) -> Result<Analysis<'a, L, Verified>, Rejected<'a, L>> {
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
        let unseeded = self.quotient.unseeded();
        if !unseeded.is_empty() {
            let message = fmt::from_fn(|f| separated(f, &unseeded, "; ")).to_string();
            return Err(Rejected {
                error: SyntaxError::new(message),
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

impl<'a, L: Language> Analysis<'a, L, Verified> {
    pub(super) fn execution_order(&self) -> &ExecutionOrder<'a, L> {
        &self.quotient.components
    }
}

/// An [`Analysis`] whose program failed [verification](Analysis::verify),
/// along with why.
///
/// Deliberately not convertible into a [`SyntaxError`] by `?`: an impl of
/// `From` for it makes `?` ambiguous wherever the error type is inferred.
/// Take [`error`](Self::error) instead.
pub struct Rejected<'a, L: Language> {
    pub error: SyntaxError,
    /// Boxed to keep the error path of a verification small.
    pub analysis: Box<Analysis<'a, L>>,
}

/// Shows the error only, so that a rejection can be `unwrap`ped.
impl<L: Language> fmt::Debug for Rejected<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rejected")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

/// Shows nothing of the graphs, which [`dot`](Analysis::dot) draws instead,
/// so that a verification can be `unwrap`ped either way.
impl<L: Language, S> fmt::Debug for Analysis<'_, L, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Analysis").finish_non_exhaustive()
    }
}

impl<'a, L: Language, S> Analysis<'a, L, S> {
    /// The components, each after every component it depends on, which is the
    /// order a [`Verified`] analysis executes them in.
    pub(super) fn components(&self) -> impl Iterator<Item = ComponentView<'_, 'a, L>> {
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
    pub(super) fn edges(&self) -> impl Iterator<Item = DependencyEdge<'a, L>> {
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
pub(super) struct ComponentView<'g, 'a, L: Language> {
    /// Its position in the order of [`components`](Analysis::components).
    pub(super) position: usize,
    pub(super) component: &'g PredicateComponent<'a, L>,
    /// Depends on no other component, that is, an input of the dataflow.
    pub(super) is_source: bool,
    /// No other component depends on it, that is, an output of the dataflow.
    pub(super) is_sink: bool,
}

/// A dependency of predicate `from` on predicate `to`, which `from` mentions
/// in one of its body's atoms.
pub(super) struct DependencyEdge<'a, L: Language> {
    pub(super) from: &'a Predicate<L>,
    pub(super) to: &'a Predicate<L>,
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
///
/// A component contains one or multiple [`Predicate`]s. In the latter case,
/// the predicates reference each other (mutual recursion) and therefore form
/// a component.
#[derive(Debug, Clone)]
pub struct PredicateComponent<'a, L: Language> {
    /// All predicates within the SCC.
    members: Vec<&'a Predicate<L>>,
    /// Edges within a component.
    intra_adjacency: Adjacency<EdgeLabel>,
}

impl<L: Language> PredicateComponent<'_, L> {
    pub(super) fn members(&self) -> impl Iterator<Item = &Predicate<L>> {
        self.members.iter().copied()
    }

    /// All rules that contribute to the definition of any of the members.
    pub(super) fn rules(&self) -> impl Iterator<Item = &Rule<L>> {
        self.members
            .iter()
            .copied()
            .flat_map(|member| member.rules())
    }

    /// Returns `Some(Predicate)` if the component contains an EDB predicate.
    /// Otherwise, `None` is returned. A component representing an EDB predicate
    /// must only contain a single [predicate](Self::members) for which
    /// [Predicate::is_edb_predicate()] is `true`. If these conditions are met,
    /// this function returns exactly that predicate.
    pub(super) fn is_edb_component(&self) -> Option<&Predicate<L>> {
        let mut iter = self.members();
        if let Some(predicate) = iter.next()
            && predicate.is_edb_predicate()
            && iter.next().is_none()
        {
            Some(predicate)
        } else {
            None
        }
    }

    /// The rules referencing a member of this component. Those are what make
    /// the component recursive, whether a rule references the very predicate it
    /// defines (self recursion) or another member (mutual recursion).
    ///
    /// [Self::rec_rules] and [Self::non_rec_rules] partition all of
    /// the component's rules into two partitions.
    pub(super) fn rec_rules(&self) -> impl Iterator<Item = &Rule<L>> {
        self.rules().filter(|rule| self.references_member(rule))
    }

    /// The rules referencing no member of this component. They only read
    /// predicates of earlier components and base tables from the EDB, all of
    /// which are fully computed by the time this component runs.
    ///
    /// [Self::non_rec_rules] and [Self::rec_rules] partition all of
    /// the component's rules into two partitions.
    pub(super) fn non_rec_rules(&self) -> impl Iterator<Item = &Rule<L>> {
        self.rules().filter(|rule| !self.references_member(rule))
    }

    /// If the component has to be evaluated recursively at all. True for every
    /// component of more than one member, and for a single member that is
    /// self-recursive.
    pub(super) fn is_recursive(&self) -> bool {
        self.rec_rules().next().is_some()
    }

    pub(super) fn contains_self_recursion(&self) -> bool {
        self.members()
            .any(|predicate| predicate.is_self_recursive())
    }

    pub(super) fn references_member(&self, rule: &Rule<L>) -> bool {
        self.members().any(|member| rule.references(member.id()))
    }
}

/// A graph in which every vertex is a [Predicate] and each
/// edge `p1 -> p2`, whenever `p1` depends on `p2`, that is, `p1` mentions `p2`
/// in one of its body's [atoms](Atom).
struct PredicateDependencyGraph<'a, L: Language> {
    predicates: IndexMap<&'a L::PredId, &'a Predicate<L>>,
    /// Predicates declared under a name an earlier predicate already took.
    /// They are no vertices, so that every name resolves to one predicate.
    duplicates: Vec<&'a Predicate<L>>,
    adjacency: Adjacency<EdgeLabel>,
}

impl<'a, L: Language> PredicateDependencyGraph<'a, L> {
    /// Builds the graph, leaving out what [`malformations`] reports: a
    /// duplicate declaration is no vertex, and an atom referencing no
    /// predicate is no edge.
    ///
    /// [`malformations`]: Self::malformations
    fn from_logical_program<LP: LogicalProgram<Lang = L>>(program: &'a LP) -> Self {
        // A predicate's position in `predicates` is its node id in the graphs
        // computed later, and the map's keys are what atoms are tested against
        // to compute the dependencies.
        // The first declaration of a name wins, the ones after it are kept
        // aside for verification to report.
        let mut predicates: IndexMap<&'a L::PredId, &'a Predicate<L>> = IndexMap::new();
        let mut duplicates: Vec<&'a Predicate<L>> = Vec::new();
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
                    .flat_map(|rule| rule.body().atoms())
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
    /// predicate, atoms whose bindings do not fit the arity of the predicate
    /// they reference, and variables no positive atom binds. See
    /// [`Malformation`].
    ///
    /// A head is checked against the predicate whose rule it heads.
    fn malformations(&self) -> Vec<Malformation<'a, L>> {
        let duplicates = self
            .duplicates
            .iter()
            .copied()
            .map(|predicate| Malformation::DuplicateDeclaration { predicate });
        let rules = self.predicates.values().copied().flat_map(|predicate| {
            predicate.rules().flat_map(move |rule| {
                let site = Site { predicate, rule };
                site.head_malformations()
                    .chain(site.unbound_variables())
                    .chain(self.body_malformations(site))
            })
        });
        duplicates.chain(rules).collect()
    }

    /// What is wrong with the body atoms of `site`'s rule: each has to
    /// reference a predicate, and fit that predicate's arity.
    fn body_malformations(&self, site: Site<'a, L>) -> impl Iterator<Item = Malformation<'a, L>> {
        site.rule.body().atoms().flat_map(move |atom| {
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
    fn predicate(&self, idx: NodeIdx) -> &'a Predicate<L> {
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
struct QuotientGraph<'a, L: Language> {
    /// The nodes of the quotient graph are the components and stored in
    /// reverse topological order.
    components: Vec<PredicateComponent<'a, L>>,
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

impl<'a, L: Language> QuotientGraph<'a, L> {
    fn from_predicate_dep_graph(graph: &PredicateDependencyGraph<'a, L>) -> Self {
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
            Vec<PredicateComponent<L>>,
        ) = components
            .iter()
            .map(|component| {
                let inter_edges_neighbors: Neighbors<EdgeLabel> = Vec::new();
                let intra_edges_adjacency: Adjacency<EdgeLabel> =
                    (0..component.len()).map(|_| Vec::new()).collect();
                let members = component
                    .iter()
                    .map(|member| graph.predicate(*member))
                    .collect::<Vec<&Predicate<L>>>();
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

        #[cfg(debug_assertions)]
        basic_component_check(&vertex_degrees);
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

    /// The components no rule seeds, see [`Unseeded`]. An EDB component is
    /// seeded by its declaring (empty) rule, and a non-recursive one by all of
    /// its rules.
    fn unseeded(&self) -> Vec<Unseeded<'_, 'a, L>> {
        self.components
            .iter()
            .filter(|component| component.non_rec_rules().next().is_none())
            .map(Unseeded)
            .collect()
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
struct NegativeCycle<'a, L: Language> {
    /// The predicate whose definition contains the offending atom.
    predicate: &'a Predicate<L>,
    /// The rule the negated atom sits in.
    rule: &'a Rule<L>,
    /// The negated name, a member of `predicate`'s own component.
    identifier: &'a L::PredId,
}

impl<L: Language> fmt::Display for NegativeCycle<'_, L> {
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

/// A component without a base case, as reported by
/// [`QuotientGraph::unseeded`]: every rule of its members reads a member, so
/// its iteration starts out empty and never derives a row.
///
/// That is valid Datalog, whose least fixed point is merely empty, but no one
/// writes a relation that is empty by construction on purpose. A forgotten base
/// rule is far likelier, which is why such a program is rejected. A predicate
/// that is declared but has no rule at all is reported the same way.
struct Unseeded<'g, 'a, L: Language>(&'g PredicateComponent<'a, L>);

impl<L: Language> fmt::Display for Unseeded<'_, '_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.members.as_slice() {
            [member] => write!(
                f,
                "predicate '{}' has no base case: none of its rules derives a row \
                 without reading it",
                member.id()
            ),
            members => {
                f.write_str("component {")?;
                separated(
                    f,
                    members.iter().map(|member| format!("'{}'", member.id())),
                    ", ",
                )?;
                f.write_str(
                    "} has no base case: none of its rules derives a row without reading \
                     one of its members",
                )
            }
        }
    }
}

/// A rule, along with the predicate it belongs to, for a diagnostic to point at.
struct Site<'a, L: Language> {
    predicate: &'a Predicate<L>,
    rule: &'a Rule<L>,
}

// Derived, they would demand `L: Clone` and `L: Copy`.
impl<L: Language> Clone for Site<'_, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Language> Copy for Site<'_, L> {}

impl<'a, L: Language> Site<'a, L> {
    /// What is wrong with the rule's head: it has to fit the arity of the
    /// predicate it belongs to, and fill each of its columns.
    fn head_malformations(self) -> impl Iterator<Item = Malformation<'a, L>> {
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

    /// The variables the rule uses but leaves unbound, each reported once per
    /// [`Occurrence`]. Only a positive body atom binds a variable, which
    /// makes the rule
    ///
    /// - _range restricted_ if each variable of its head is bound,
    /// - _safe_ in its negation if each variable of a negated atom is bound,
    /// - and _safe_ in its comparisons if each variable of a condition is bound.
    ///
    /// Strict for negation: a variable only a negated atom mentions is not
    /// read as existential. A frontend wanting any value there leaves the
    /// position unbound instead, so that a misspelled variable cannot silently
    /// turn into one.
    ///
    /// An EDB predicate's rule is exempt, as it declares the relation rather
    /// than deriving it, and its head binds nothing.
    fn unbound_variables(self) -> Vec<Malformation<'a, L>> {
        if self.predicate.is_edb_predicate() {
            return Vec::new();
        }
        let rule = self.rule;
        let body = rule.body();
        let occurrences = iter::once((Occurrence::Head, rule.head().vars().collect::<Vec<_>>()))
            .chain(
                body.negated_atoms()
                    .map(|atom| (Occurrence::Negated(atom), atom.vars().collect())),
            )
            .chain(
                body.conditions()
                    .map(|cond| (Occurrence::Condition(cond), cond.vars().collect())),
            );
        occurrences
            .flat_map(|(occurrence, vars)| {
                let mut reported = HashSet::new();
                vars.into_iter()
                    .filter(move |var| {
                        !body.bound_vars().contains_key(var.id()) && reported.insert(var.id())
                    })
                    .map(move |var| Malformation::Unbound {
                        site: self,
                        var,
                        occurrence,
                    })
            })
            .collect()
    }
}

/// Where a rule uses a variable that only a positive body atom can bind.
enum Occurrence<'a, L: Language> {
    Head,
    Negated(&'a Atom<L>),
    Condition(&'a Cond<L>),
}

// Derived, they would demand `L: Clone` and `L: Copy`.
impl<L: Language> Clone for Occurrence<'_, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Language> Copy for Occurrence<'_, L> {}

/// The bindings of `atom`, which `site`'s rule contains, that do not fit
/// `referenced`, the predicate it references: positions beyond its arity, and
/// positions bound before.
fn misplaced_bindings<'a, L: Language>(
    site: Site<'a, L>,
    atom: &'a Atom<L>,
    referenced: &'a Predicate<L>,
) -> impl Iterator<Item = Malformation<'a, L>> {
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

/// What makes a program ill-formed, as reported by
/// [`PredicateDependencyGraph::malformations`]. Borrows the offending site
/// from the program so that a diagnostic can point at it.
///
/// Unlike a [`NegativeCycle`], a malformation is visible without the graphs,
/// but it leaves them incomplete: a duplicate declaration is no vertex, and a
/// dangling atom no edge.
enum Malformation<'a, L: Language> {
    /// A predicate declared under a name an earlier predicate already took.
    DuplicateDeclaration { predicate: &'a Predicate<L> },
    /// A body atom referencing a name no predicate is declared under.
    Dangling {
        site: Site<'a, L>,
        atom: &'a Atom<L>,
    },
    /// An atom binding a position its predicate has no column at.
    PositionOutOfRange {
        site: Site<'a, L>,
        atom: &'a Atom<L>,
        position: usize,
        arity: usize,
    },
    /// An atom binding a position more than once.
    PositionRepeated {
        site: Site<'a, L>,
        atom: &'a Atom<L>,
        position: usize,
    },
    /// A head leaving a column of its predicate unfilled, with nothing to
    /// project into it.
    ColumnUnfilled {
        site: Site<'a, L>,
        position: usize,
        column: &'a Column,
    },
    /// A variable no positive body atom binds, used where it has to be bound.
    /// See [`Site::unbound_variables`].
    Unbound {
        site: Site<'a, L>,
        var: &'a L::Var,
        occurrence: Occurrence<'a, L>,
    },
}

impl<L: Language> fmt::Display for Malformation<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let in_rule = |f: &mut fmt::Formatter<'_>, site: &Site<'_, L>| {
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
            Malformation::Unbound {
                site,
                var,
                occurrence,
            } => {
                in_rule(f, site)?;
                write!(f, "uses '{}' ", var.name())?;
                match occurrence {
                    Occurrence::Head => f.write_str("in its head")?,
                    Occurrence::Negated(atom) => write!(f, "in negated '{}'", atom.id())?,
                    Occurrence::Condition(cond) => write!(f, "in '{cond}'")?,
                }
                f.write_str(", which no positive atom binds")
            }
        }
    }
}

#[cfg(debug_assertions)]
fn basic_component_check(vertex_degrees: &[VertexDegree]) {
    if !vertex_degrees.is_empty() {
        debug_assert!(
            vertex_degrees.first().expect("bounds checked").out_degree == 0,
            "first SCC must be a source by construction"
        );
        debug_assert!(
            vertex_degrees.last().expect("bounds checked").in_degree == 0,
            "last SCC must be a sink by construction"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::{LogicalProgram, test_utils::*};
    use super::*;

    #[test]
    fn a_component_may_have_more_members_than_the_program_has_components() {
        // One component of three members, and only the one EDB relation that
        // seeds it as a component of its own.
        let program = program(vec![
            pred("e", &[&[]]),
            pred("p", &[&["q"], &["e"]]),
            pred("q", &[&["r"]]),
            pred("r", &[&["p"]]),
        ]);
        let analysis = analyze(&program)
            .verify()
            .expect("a seeded positive cycle is valid");
        assert_eq!(analysis.execution_order().len(), 2);
    }

    #[test]
    fn a_head_has_to_fill_exactly_its_predicate_s_columns() {
        let predicates = Predicate::group(
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
        let predicates = Predicate::group(
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
    fn a_head_variable_has_to_be_bound_by_a_positive_atom() {
        // `r1` uses `y` twice in its head, which is reported once. `r2` binds
        // `y` in a negated atom only, which does not count.
        let predicates = Predicate::group(
            [declare("edge", &["x", "y"]), declare("path", &["x", "y"])],
            [
                rule("e0", "edge(x, y)", &[]),
                rule("r0", "path(x, y)", &["edge(x, z)"]),
                rule("r1", "path(y, y)", &["edge(x, x)"]),
                rule("r2", "path(x, y)", &["edge(x, x)", "!edge(x, y)"]),
            ],
        )
        .expect("every definand names a declared predicate");
        let program = program(predicates);
        let rejected = program
            .verify()
            .expect_err("every rule of `path` leaves a head variable unbound");
        assert_eq!(
            rejected.error.message(),
            "rule 'r0' of predicate 'path' uses 'y' in its head, which no positive atom binds; \
             rule 'r1' of predicate 'path' uses 'y' in its head, which no positive atom binds; \
             rule 'r2' of predicate 'path' uses 'y' in its head, which no positive atom binds; \
             rule 'r2' of predicate 'path' uses 'y' in negated 'edge', which no positive atom binds"
        );
    }

    #[test]
    fn a_negated_variable_has_to_be_bound_by_a_positive_atom() {
        // `z` is not read as existential, as the strict reading of safety
        // demands. `_` is what leaves a position open.
        let predicates = Predicate::group(
            [declare("edge", &["x", "y"]), declare("path", &["x", "y"])],
            [
                rule("e0", "edge(x, y)", &[]),
                rule("r0", "path(x, y)", &["edge(x, y)", "!edge(y, z)"]),
                rule("r1", "path(x, y)", &["edge(x, y)", "!edge(y, _)"]),
            ],
        )
        .expect("every definand names a declared predicate");
        let program = program(predicates);
        let rejected = program
            .verify()
            .expect_err("`r0` negates an unbound variable");
        assert_eq!(
            rejected.error.message(),
            "rule 'r0' of predicate 'path' uses 'z' in negated 'edge', which no positive atom binds"
        );
    }

    #[test]
    fn a_condition_s_variable_has_to_be_bound_by_a_positive_atom() {
        // A condition binds nothing, not even an equality: `r1`'s `y` stays
        // unbound in its head, too.
        let predicates = Predicate::group(
            [declare("edge", &["x", "y"]), declare("path", &["x", "y"])],
            [
                rule("e0", "edge(x, y)", &[]),
                rule("r0", "path(x, y)", &["edge(x, y)", "x < z"]),
                rule("r1", "path(x, y)", &["edge(x, _)", "y == x"]),
            ],
        )
        .expect("every definand names a declared predicate");
        let program = program(predicates);
        let rejected = program
            .verify()
            .expect_err("every rule of `path` constrains an unbound variable");
        assert_eq!(
            rejected.error.message(),
            "rule 'r0' of predicate 'path' uses 'z' in 'x < z', which no positive atom binds; \
             rule 'r1' of predicate 'path' uses 'y' in its head, which no positive atom binds; \
             rule 'r1' of predicate 'path' uses 'y' in 'y == x', which no positive atom binds"
        );
    }

    #[test]
    fn only_a_rule_with_an_empty_body_declares_an_edb_predicate() {
        // `edge`'s head binds nothing, which is fine for a declaration. `small`
        // has a condition, so it is derived, and its head has to be bound.
        let predicates = Predicate::group(
            [declare("edge", &["x", "y"]), declare("small", &["x"])],
            [
                rule("e0", "edge(x, y)", &[]),
                rule("s0", "small(x)", &["x < 3"]),
            ],
        )
        .expect("every definand names a declared predicate");
        let program = program(predicates);
        let rejected = program.verify().expect_err("`small` is no EDB predicate");
        assert_eq!(
            rejected.error.message(),
            "rule 's0' of predicate 'small' uses 'x' in its head, which no positive atom binds; \
             rule 's0' of predicate 'small' uses 'x' in 'x < 3', which no positive atom binds"
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
    fn a_predicate_reading_only_itself_has_no_base_case() {
        // Valid Datalog, `p` is merely empty, and so almost certainly lacks a
        // rule that was meant to seed it.
        let program = program(vec![pred("e", &[&[]]), pred("p", &[&["p", "e"]])]);
        let rejected = program.verify().expect_err("nothing seeds `p`");
        assert_eq!(
            rejected.error.message(),
            "predicate 'p' has no base case: none of its rules derives a row without reading it"
        );
    }

    #[test]
    fn a_component_has_no_base_case_if_none_of_its_members_has_one() {
        // `q` has no base rule of its own, which is fine as long as `p` has:
        // `q` is seeded through `p`. Without `p`'s base rule, neither is.
        let seeded = program(vec![
            pred("e", &[&[]]),
            pred("p", &[&["q"], &["e"]]),
            pred("q", &[&["p"]]),
        ]);
        seeded
            .verify()
            .expect("`p`'s base rule seeds the component");

        let unseeded = program(vec![
            pred("e", &[&[]]),
            pred("p", &[&["q", "e"]]),
            pred("q", &[&["p"]]),
        ]);
        let rejected = unseeded.verify().expect_err("nothing seeds `p` and `q`");
        assert_eq!(
            rejected.error.message(),
            "component {'p', 'q'} has no base case: none of its rules derives a row without \
             reading one of its members"
        );
    }

    #[test]
    fn a_predicate_without_any_rule_has_no_base_case() {
        let program = program(vec![pred("p", &[])]);
        let rejected = program.verify().expect_err("`p` is never defined");
        assert_eq!(
            rejected.error.message(),
            "predicate 'p' has no base case: none of its rules derives a row without reading it"
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
