// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct VertexDegree {
    pub in_degree: u32,
    pub out_degree: u32,
}

pub(super) fn vertex_degrees(adjacency: &[Vec<usize>]) -> Vec<VertexDegree> {
    let mut degrees = vec![VertexDegree::default(); adjacency.len()];
    for (from, neighbors) in adjacency.iter().enumerate() {
        degrees[from].out_degree =
            u32::try_from(neighbors.len()).expect("we should not have such big graphs");
        for to in neighbors {
            degrees[*to].in_degree += 1;
        }
    }
    degrees
}

/// Gabow's path-based strong component algorithm.
///
/// The algorithm operates on a graph given as an adjacency list over dense
/// node indices; it knows nothing about what the nodes stand for. Callers map
/// the resulting indices back onto their own data.
pub(super) mod gabow {
    use std::slice;

    /// Computes the strongly connected components of `adjacency` in reverse
    /// topological order.
    ///
    /// Node `i` is `adjacency[i]`, listing the nodes `i` has an edge to. Read as
    /// a dependency graph: An edge `i -> j` meaning "`i` depends on `j`". The
    /// returned components are in execution order: a component is emitted only
    /// after every component it (transitively) depends on. Within a component,
    /// nodes appear in the order the depth-first search discovered them.
    ///
    /// A node without any cycle through it forms a component of its own, so the
    /// result is a partition of `0..adjacency.len()`. Self loops are not special
    /// cased: a node with an edge to itself also yields a single-member component
    /// and callers that care have to check for the self reference themselves.
    pub fn sccs(adjacency: &[Vec<usize>]) -> Sccs {
        let mut search = Search::new(adjacency);
        (0..adjacency.len()).for_each(|root| search.run(root));
        Sccs {
            component_of: search
                .component
                .into_iter()
                .map(|component| component.expect("the search decides every node"))
                .collect(),
            components: search.sccs,
        }
    }

    /// The strongly connected components of a graph, in reverse topological order,
    /// together with a lookup table to tell which component each node ended up in.
    pub struct Sccs {
        pub components: Vec<Vec<usize>>,
        /// Indexed by node, holding an index into `components`. A [`Vec`] rather
        /// than a map because the components partition `0..adjacency.len()`, so
        /// every node is a slot and no key is ever absent.
        pub component_of: Vec<usize>,
    }

    /// One entry of the explicit depth-first stack. The search is iterative rather
    /// than recursive so that long chains of nodes cannot overflow the call stack.
    struct Frame<'a> {
        node: usize,
        /// The successors of `node` that have not been walked yet.
        successors: slice::Iter<'a, usize>,
    }

    struct Search<'a> {
        adjacency: &'a [Vec<usize>],
        /// Preorder number per node, `None` until the node is first reached.
        preorder: Vec<Option<usize>>,
        /// The component a node ended up in, `None` while it is still undecided.
        component: Vec<Option<usize>>,
        /// Hands out the next preorder number.
        counter: usize,
        /// Gabow's `S`: every node reached but not yet assigned to a component,
        /// in preorder.
        path: Vec<usize>,
        /// Gabow's `P`: the nodes on `path` that may still turn out to be the
        /// root of a component. A back edge contracts everything it reaches over.
        roots: Vec<usize>,
        stack: Vec<Frame<'a>>,
        sccs: Vec<Vec<usize>>,
    }

    impl<'a> Search<'a> {
        fn new(adjacency: &'a [Vec<usize>]) -> Self {
            Self {
                adjacency,
                preorder: vec![None; adjacency.len()],
                component: vec![None; adjacency.len()],
                counter: 0,
                path: Vec::new(),
                roots: Vec::new(),
                stack: Vec::new(),
                sccs: Vec::new(),
            }
        }

        /// Explores everything reachable from `root` that has not been seen yet.
        fn run(&mut self, root: usize) {
            if self.preorder[root].is_some() {
                return;
            }
            self.discover(root);
            while let Some(frame) = self.stack.last_mut() {
                let node = frame.node;
                match frame.successors.next().copied() {
                    Some(successor) => self.walk(successor),
                    None => {
                        self.stack.pop();
                        self.close(node);
                    }
                }
            }
        }

        /// Reaches `successor` over an edge from the node currently on top of the
        /// depth-first stack.
        fn walk(&mut self, successor: usize) {
            match (self.preorder[successor], self.component[successor]) {
                // A tree edge into unexplored territory.
                (None, _) => self.discover(successor),
                // A back or cross edge into an undecided node: everything reached
                // over it lies on one cycle and hence in one component, so none of
                // those nodes can be a component root but the oldest one.
                (Some(preorder), None) => self.contract(preorder),
                // An edge into a finished component, which tells us nothing.
                (Some(_), Some(_)) => {}
            }
        }

        fn discover(&mut self, node: usize) {
            self.preorder[node] = Some(self.counter);
            self.counter += 1;
            self.path.push(node);
            self.roots.push(node);
            self.stack.push(Frame {
                node,
                successors: self.adjacency[node].iter(),
            });
        }

        /// Drops every candidate root that is younger than `preorder`.
        fn contract(&mut self, preorder: usize) {
            while self
                .roots
                .last()
                .is_some_and(|&root| self.preorder_of(root) > preorder)
            {
                self.roots.pop();
            }
        }

        /// Finishes `node`. If it survived as a candidate root, it is the oldest
        /// node of its component and everything stacked on top of it on `path`
        /// belongs to that same component.
        fn close(&mut self, node: usize) {
            if self.roots.last() != Some(&node) {
                return;
            }
            self.roots.pop();
            let start = self
                .path
                .iter()
                .rposition(|&member| member == node)
                .expect("a candidate root is still on the path");
            let members = self.path.split_off(start);
            let component = self.sccs.len();
            members
                .iter()
                .for_each(|&member| self.component[member] = Some(component));
            self.sccs.push(members);
        }

        fn preorder_of(&self, node: usize) -> usize {
            self.preorder[node].expect("a reached node has a preorder number")
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Checks that `sccs` partitions the nodes, that no component is emitted
        /// before one it has an edge into, and that the lookup table agrees with
        /// the components it is derived from.
        fn assert_well_formed(adjacency: &[Vec<usize>], sccs: &Sccs) {
            let mut component_of = vec![None; adjacency.len()];
            sccs.components
                .iter()
                .enumerate()
                .for_each(|(idx, members)| {
                    members.iter().for_each(|&member| {
                        assert_eq!(
                            component_of[member], None,
                            "node {member} in two components"
                        );
                        component_of[member] = Some(idx);
                    })
                });
            adjacency.iter().enumerate().for_each(|(node, successors)| {
                let node_component = component_of[node].expect("every node is in a component");
                assert_eq!(
                    sccs.component_of[node], node_component,
                    "node {node} looks up the wrong component"
                );
                successors.iter().for_each(|&successor| {
                    let successor_component = component_of[successor].expect("in a component");
                    assert!(
                        successor_component <= node_component,
                        "{successor} is emitted after its dependent {node}"
                    );
                })
            });
        }

        fn sccs_of(adjacency: &[Vec<usize>]) -> Vec<Vec<usize>> {
            let sccs = sccs(adjacency);
            assert_well_formed(adjacency, &sccs);
            sccs.components
        }

        #[test]
        fn empty_graph() {
            assert_eq!(sccs_of(&[]), Vec::<Vec<usize>>::new());
        }

        #[test]
        fn isolated_nodes() {
            assert_eq!(
                sccs_of(&[vec![], vec![], vec![]]),
                vec![vec![0], vec![1], vec![2]]
            );
        }

        #[test]
        fn self_loop_is_a_single_member_component() {
            assert_eq!(sccs_of(&[vec![0]]), vec![vec![0]]);
        }

        #[test]
        fn chain_is_emitted_back_to_front() {
            // 0 -> 1 -> 2, so 2 has to run first.
            assert_eq!(
                sccs_of(&[vec![1], vec![2], vec![]]),
                vec![vec![2], vec![1], vec![0]]
            );
        }

        #[test]
        fn mutual_pair_forms_one_component() {
            assert_eq!(sccs_of(&[vec![1], vec![0]]), vec![vec![0, 1]]);
        }

        #[test]
        fn cycle_with_a_dependency_below_it() {
            // 0 <-> 1, both depending on the non-recursive 2.
            let adjacency = [vec![1, 2], vec![0, 2], vec![]];
            assert_eq!(sccs_of(&adjacency), vec![vec![2], vec![0, 1]]);
        }

        #[test]
        fn diamond_keeps_both_sides_apart() {
            // 0 -> {1, 2} -> 3.
            let adjacency = [vec![1, 2], vec![3], vec![3], vec![]];
            assert_eq!(
                sccs_of(&adjacency),
                vec![vec![3], vec![1], vec![2], vec![0]]
            );
        }

        #[test]
        fn disjoint_cycles_stay_separate() {
            let adjacency = [vec![1], vec![0], vec![3], vec![2]];
            assert_eq!(sccs_of(&adjacency), vec![vec![0, 1], vec![2, 3]]);
        }

        #[test]
        fn nested_cycles_collapse_into_one_component() {
            // 0 -> 1 -> 2 -> 0 with a shortcut 1 -> 0 and a self loop on 2.
            let adjacency = [vec![1], vec![2, 0], vec![0, 2]];
            assert_eq!(sccs_of(&adjacency), vec![vec![0, 1, 2]]);
        }

        #[test]
        fn cross_edge_into_a_finished_component() {
            // 0 -> 1 <-> 2 and 3 -> 1, reached from a later root.
            let adjacency = [vec![1], vec![2], vec![1], vec![1]];
            assert_eq!(sccs_of(&adjacency), vec![vec![1, 2], vec![0], vec![3]]);
        }

        #[test]
        fn long_chain_does_not_overflow_the_stack() {
            let len = 200_000;
            let adjacency: Vec<Vec<usize>> = (0..len)
                .map(|node| {
                    if node + 1 < len {
                        vec![node + 1]
                    } else {
                        vec![]
                    }
                })
                .collect();
            let sccs = sccs(&adjacency).components;
            assert_eq!(sccs.len(), len);
            assert_eq!(sccs.first(), Some(&vec![len - 1]));
            assert_eq!(sccs.last(), Some(&vec![0]));
        }

        #[test]
        fn long_cycle_does_not_overflow_the_stack() {
            let len = 200_000;
            let adjacency: Vec<Vec<usize>> = (0..len).map(|node| vec![(node + 1) % len]).collect();
            let sccs = sccs(&adjacency).components;
            assert_eq!(sccs.len(), 1);
            assert_eq!(sccs[0], (0..len).collect::<Vec<_>>());
        }
    }
}
