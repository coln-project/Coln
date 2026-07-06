// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module is feature-gated behind `test`. It only contains raw DBSP tests
//! and experiments without any wrappers from our end.

mod utils {
    use dbsp::ZWeight;
    use dbsp::utils::Tup2;
    use std::io::{self, IsTerminal, Write};
    use std::num::NonZeroUsize;

    pub fn worker_threads() -> NonZeroUsize {
        std::thread::available_parallelism().unwrap_or(NonZeroUsize::new(4).unwrap())
    }

    /// Iterator adapter that waits for a keypress before yielding each item if in
    /// interactive mode. Useful for observing an incremental computation step by step.
    pub struct Confirm<Iter> {
        iter: Iter,
        interactive: bool,
    }

    impl<Iter> Confirm<Iter> {
        fn new(iter: Iter, interactive: bool) -> Self {
            Self { iter, interactive }
        }
    }

    // If the underlying Iter is an ExactSizeIterator make Confirm<Iter>
    // be one, too.
    impl<Iter: ExactSizeIterator> ExactSizeIterator for Confirm<Iter> {}

    impl<Iter: Iterator> Iterator for Confirm<Iter> {
        type Item = Iter::Item;

        fn next(&mut self) -> Option<Self::Item> {
            if !self.interactive {
                return self.iter.next();
            }
            print!("Press Enter to continue (Ctrl-D to stop)...");
            io::stdout().flush().ok();
            let mut line = String::new();
            match io::stdin().read_line(&mut line) {
                Ok(0) | Err(_) => None, // EOF / Ctrl-D -> stop
                Ok(_) => self.iter.next(),
            }
        }

        fn size_hint(&self) -> (usize, Option<usize>) {
            self.iter.size_hint()
        }
    }

    /// Whether we're in an interactive `cargo test -- --nocapture`
    /// and run on a real TTY.
    fn confirmation_enabled() -> bool {
        let nocapture = || {
            std::env::args().any(|a| a == "--nocapture")
                || std::env::var_os("RUST_TEST_NOCAPTURE").is_some()
        };
        io::stdin().is_terminal() && nocapture()
    }

    /// Blanket extension trait with a default impl on every `Iterator`.
    pub trait ConfirmIterExt: Iterator + Sized {
        fn confirm_each_auto(self) -> Confirm<Self> {
            Confirm::new(self, confirmation_enabled())
        }
        fn confirm_each(self, interactive: bool) -> Confirm<Self> {
            Confirm::new(self, interactive)
        }
    }

    // Blanket implementation for Iterators.
    impl<Iter: Iterator> ConfirmIterExt for Iter {}

    pub struct Bounded {
        bound: usize,
        delim: char,
    }
    impl Bounded {
        fn new(bound: usize) -> Self {
            Self { bound, delim: '/' }
        }
    }
    pub struct Unbounded;

    /// Iterator adaptor that displays the progress for an iterator.
    pub struct Progress<Iter, Bound> {
        iter: Iter,
        count: usize,
        prefix: char,
        suffix: char,
        bound: Bound,
    }

    impl<Iter> Progress<Iter, Unbounded> {
        fn new(iter: Iter) -> Self {
            Self {
                iter,
                count: 0,
                prefix: '\n',
                suffix: ' ',
                bound: Unbounded,
            }
        }
        pub fn with_delims(mut self, prefix: char, suffix: char) -> Self {
            self.prefix = prefix;
            self.suffix = suffix;
            self
        }
    }

    impl<Iter: ExactSizeIterator> Progress<Iter, Unbounded> {
        pub fn with_bound(self) -> Progress<Iter, Bounded> {
            let bound = self.iter.len();
            Progress {
                iter: self.iter,
                count: self.count,
                prefix: self.prefix,
                suffix: self.suffix,
                bound: Bounded::new(bound),
            }
        }
    }

    impl<Iter> Progress<Iter, Bounded> {
        pub fn with_delims(
            mut self,
            progress_prefix: char,
            bounded_delim: char,
            progress_suffix: char,
        ) -> Self {
            self.prefix = progress_prefix;
            self.suffix = progress_suffix;
            self.bound.delim = bounded_delim;
            self
        }
    }

    trait ProgressDisplay
    where
        Self: Sized,
    {
        fn display<Iter>(&self, progress: &Progress<Iter, Self>) -> impl std::fmt::Display;
    }

    impl ProgressDisplay for Bounded {
        fn display<Iter>(&self, progress: &Progress<Iter, Self>) -> impl std::fmt::Display {
            if progress.count > self.bound {
                return "DONE".to_string();
            }
            format!("{}{}{}", progress.count, self.delim, self.bound)
        }
    }

    impl ProgressDisplay for Unbounded {
        fn display<Iter>(&self, progress: &Progress<Iter, Self>) -> impl std::fmt::Display {
            format!("{}", progress.count)
        }
    }

    // If the underlying Iter is an ExactSizeIterator make Progress<Iter, Bound>
    // be one, too.
    impl<Iter: ExactSizeIterator, Bound: ProgressDisplay> ExactSizeIterator for Progress<Iter, Bound> {}

    impl<Iter, Bound> Iterator for Progress<Iter, Bound>
    where
        Iter: Iterator,
        Bound: ProgressDisplay,
    {
        type Item = Iter::Item;

        fn next(&mut self) -> Option<Self::Item> {
            self.count += 1;
            print!("{}{}{}", self.prefix, self.bound.display(self), self.suffix);
            self.iter.next()
        }

        fn size_hint(&self) -> (usize, Option<usize>) {
            self.iter.size_hint()
        }
    }

    pub trait ProgressIterExt<Bound>: Iterator + Sized {
        fn progress(self) -> Progress<Self, Bound>;
    }

    // Blanket implementation for Iterators.
    impl<Iter: Iterator> ProgressIterExt<Unbounded> for Iter {
        fn progress(self) -> Progress<Self, Unbounded> {
            Progress::new(self)
        }
    }

    pub type ZWeightElement<K> = Tup2<K, ZWeight>;
    pub type ZWeightIndexElement<K, V> = Tup2<K, Tup2<V, ZWeight>>;
    pub type Batch<K> = Vec<ZWeightElement<K>>;
    pub type IndexBatch<K, V> = Vec<ZWeightIndexElement<K, V>>;
}

use super::cli_table::ToCliTable;
use dbsp::{
    Circuit, IndexedZSetReader, NestedCircuit, OrdZSet, Runtime, Stream, ZWeight, indexed_zset,
    operator::{Generator, Z1},
    utils::{Tup2, Tup3, Tup4},
    zset,
};
use std::{cell::RefCell, rc::Rc};
use utils::{Batch, IndexBatch, worker_threads};
use utils::{ConfirmIterExt, ProgressIterExt};

// Adapted from the [DBSP docs](https://docs.rs/dbsp/latest/dbsp/circuit/circuit_builder/struct.ChildCircuit.html#method.recursive).
#[test]
fn test_recursive() -> Result<(), anyhow::Error> {
    const STEPS: usize = 3;

    // Propagate labels along graph edges.
    let (mut circuit, ((edges_input, init_labels_input), labels_output)) =
        Runtime::init_circuit(worker_threads(), move |root_circuit| {
            let (edges, edges_input) = root_circuit.add_input_zset::<Tup2<u64, u64>>();
            let (init_labels, init_labels_input) =
                root_circuit.add_input_zset::<Tup2<u64, String>>();

            let labels = root_circuit.recursive(
                |child_circuit, labels: Stream<_, OrdZSet<Tup2<u64, String>>>| {
                    // Import `edges` and `init_labels` relations from the parent circuit.
                    let edges = edges.delta0(child_circuit);
                    let init_labels = init_labels.delta0(child_circuit);

                    // Given an edge `from -> to` where the `from` node is labeled with `l`,
                    // propagate `l` to node `to`.
                    let result = labels
                        .map_index(|Tup2(x, y)| (*x, y.clone()))
                        .join(&edges.map_index(|Tup2(x, y)| (*x, *y)), |_from, l, to| {
                            Tup2(*to, l.clone())
                        })
                        .plus(&init_labels);
                    Ok(result)
                },
            )?;

            Ok(((edges_input, init_labels_input), labels.accumulate_output()))
        })?;

    // Graph topology.
    let mut edges_inputs = ([
        // Start with four nodes connected in a cycle.
        vec![
            Tup2(Tup2(1, 2), 1),
            Tup2(Tup2(2, 3), 1),
            Tup2(Tup2(3, 4), 1),
            Tup2(Tup2(4, 1), 1),
        ],
        // Add an edge.
        vec![Tup2(Tup2(4, 5), 1)],
        // Remove an edge, breaking the cycle.
        vec![Tup2(Tup2(1, 2), -1)],
    ] as [Vec<Tup2<Tup2<u64, u64>, ZWeight>>; STEPS])
        .into_iter();

    // Initial labeling of the graph.
    let mut init_labels_inputs = ([
        // Start with a single label on node 1.
        vec![Tup2(Tup2(1, "l1".to_string()), 1)],
        // Add a label to node 2.
        vec![Tup2(Tup2(2, "l2".to_string()), 1)],
        vec![],
    ] as [Vec<Tup2<Tup2<u64, String>, ZWeight>>; STEPS])
        .into_iter();

    // Expected _changes_ to the output graph labeling after each clock cycle.
    let mut label_expected_outputs = ([
        zset! {
            Tup2(1, "l1".to_string()) => 1,
            Tup2(2, "l1".to_string()) => 1,
            Tup2(3, "l1".to_string()) => 1,
            Tup2(4, "l1".to_string()) => 1
        },
        zset! {
            Tup2(1, "l2".to_string()) => 1,
            Tup2(2, "l2".to_string()) => 1,
            Tup2(3, "l2".to_string()) => 1,
            Tup2(4, "l2".to_string()) => 1,
            Tup2(5, "l1".to_string()) => 1,
            Tup2(5, "l2".to_string()) => 1
        },
        zset! {
            Tup2(2, "l1".to_string()) => -1,
            Tup2(3, "l1".to_string()) => -1,
            Tup2(4, "l1".to_string()) => -1,
            Tup2(5, "l1".to_string()) => -1
        },
    ] as [OrdZSet<Tup2<u64, String>>; STEPS])
        .into_iter();

    for _ in (0..STEPS).confirm_each_auto().progress().with_bound() {
        edges_input.append(&mut edges_inputs.next().unwrap());
        init_labels_input.append(&mut init_labels_inputs.next().unwrap());
        circuit.transaction()?;
        let labels_output = labels_output.concat();
        print!("Labels\n{}", labels_output.iter().to_cli_table());
        assert_eq!(
            labels_output.consolidate(),
            label_expected_outputs.next().unwrap()
        );
    }

    Ok(())
}

#[test]
fn test_not_operator() -> Result<(), anyhow::Error> {
    const STEPS: usize = 2;

    let (mut circuit, ((left_input, right_input), output)) =
        Runtime::init_circuit(worker_threads(), |root_circuit| {
            let (left, left_input) =
                root_circuit.add_input_indexed_zset::<Tup2<usize, usize>, Tup2<usize, usize>>();

            let (right, right_input) =
                root_circuit.add_input_indexed_zset::<Tup2<usize, usize>, Tup2<usize, usize>>();

            let set_minus = left.minus(&right);

            Ok(((left_input, right_input), set_minus.accumulate_output()))
        })?;

    let mut left_data = ([
        vec![
            Tup2(Tup2(1, 1), Tup2(Tup2(1, 1), 1)),
            Tup2(Tup2(1, 2), Tup2(Tup2(1, 2), 1)),
            Tup2(Tup2(1, 3), Tup2(Tup2(1, 3), 1)),
            Tup2(Tup2(1, 4), Tup2(Tup2(1, 4), 1)),
        ],
        vec![],
    ] as [IndexBatch<Tup2<usize, usize>, Tup2<usize, usize>>; STEPS])
        .into_iter();

    let mut right_data = ([
        vec![
            Tup2(Tup2(1, 2), Tup2(Tup2(1, 2), 1)),
            Tup2(Tup2(1, 3), Tup2(Tup2(1, 3), 1)),
        ],
        vec![Tup2(Tup2(1, 4), Tup2(Tup2(1, 4), 1))],
    ] as [IndexBatch<Tup2<usize, usize>, Tup2<usize, usize>>; STEPS])
        .into_iter();

    let mut expected_outputs = ([
        indexed_zset! {Tup2<usize, usize> => Tup2<usize, usize>:
            Tup2(1, 1) => { Tup2(1, 1) => 1 },
            Tup2(1, 4) => { Tup2(1, 4) => 1 },
        },
        indexed_zset! {Tup2<usize, usize> => Tup2<usize, usize>:
            Tup2(1, 4) => { Tup2(1, 4) => -1 },
        },
    ] as [_; STEPS])
        .into_iter();

    for i in (0..STEPS).confirm_each_auto().progress().with_bound() {
        left_input.append(&mut left_data.next().unwrap());
        right_input.append(&mut right_data.next().unwrap());
        circuit.transaction()?;
        let output = output.concat();
        print!("{}", output.iter().to_cli_table());
        assert_eq!(output.consolidate(), expected_outputs.next().unwrap());
    }

    Ok(())
}

#[test]
fn test_cartesian_product() -> Result<(), anyhow::Error> {
    const STEPS: usize = 2;

    let (mut circuit, ((left_input, right_input), output)) =
        Runtime::init_circuit(worker_threads(), |root_circuit| {
            let (left, left_input) =
                root_circuit.add_input_indexed_zset::<Tup2<usize, usize>, Tup2<usize, usize>>();
            let left = left.map_index(|(_k, v)| ((), *v));

            let (right, right_input) =
                root_circuit.add_input_indexed_zset::<Tup2<usize, usize>, Tup2<usize, usize>>();
            let right = right.map_index(|(_k, v)| ((), *v));

            let cartesian_product = left.join_index(&right, |_k, Tup2(l1, l2), Tup2(r1, r2)| {
                // Merge left and right tuples.
                Some(((), Tup4(*l1, *l2, *r1, *r2)))
            });

            Ok((
                (left_input, right_input),
                cartesian_product.accumulate_output(),
            ))
        })?;

    let mut left_data = ([
        vec![
            Tup2(Tup2(1_usize, 1_usize), Tup2(Tup2(1_usize, 1_usize), 1_i64)),
            Tup2(Tup2(1, 1), Tup2(Tup2(1, 1), 1)), // duplicate of the above!
            Tup2(Tup2(1, 2), Tup2(Tup2(1, 2), 1)),
            Tup2(Tup2(1, 3), Tup2(Tup2(1, 3), 1)),
        ],
        vec![],
    ] as [_; STEPS])
        .into_iter();

    let mut right_data = ([
        vec![
            Tup2(Tup2(2_usize, 1_usize), Tup2(Tup2(2_usize, 1_usize), 1_i64)),
            Tup2(Tup2(2, 2), Tup2(Tup2(2, 2), 1)),
        ],
        vec![Tup2(Tup2(2, 3), Tup2(Tup2(2, 3), 1))],
    ] as [_; STEPS])
        .into_iter();

    let mut expected_outputs = ([
        indexed_zset! {() => Tup4<usize, usize, usize, usize>:
            () => { Tup4(1, 1, 2, 1) => 2 },
            () => { Tup4(1, 1, 2, 2) => 2 },
            () => { Tup4(1, 2, 2, 1) => 1 },
            () => { Tup4(1, 2, 2, 2) => 1 },
            () => { Tup4(1, 3, 2, 1) => 1 },
            () => { Tup4(1, 3, 2, 2) => 1 },
        },
        indexed_zset! {() => Tup4<usize, usize, usize, usize>:
            () => { Tup4(1, 1, 2, 3) => 2 },
            () => { Tup4(1, 2, 2, 3) => 1 },
            () => { Tup4(1, 3, 2, 3) => 1 },
        },
    ] as [_; STEPS])
        .into_iter();

    for i in (0..STEPS).confirm_each_auto().progress().with_bound() {
        left_input.append(&mut left_data.next().unwrap());
        right_input.append(&mut right_data.next().unwrap());
        circuit.transaction()?;
        let output = output.concat();
        print!("{}", output.iter().to_cli_table());
        assert_eq!(output.consolidate(), expected_outputs.next().unwrap());
    }

    Ok(())
}

#[test]
fn negative_zweight_behavior() -> Result<(), anyhow::Error> {
    const STEPS: usize = 6;

    let (mut circuit, ((left_input, right_input), output)) =
        Runtime::init_circuit(worker_threads(), |root_circuit| {
            let (left, left_input) =
                root_circuit.add_input_indexed_zset::<Tup2<usize, usize>, Tup2<usize, usize>>();

            let (right, right_input) =
                root_circuit.add_input_indexed_zset::<Tup2<usize, usize>, Tup2<usize, usize>>();

            let joined = left.join_index(&right, |k, Tup2(l1, l2), Tup2(r1, r2)| {
                // Merge left and right tuples.
                Some((*k, Tup4(*l1, *l2, *r1, *r2)))
            });

            Ok(((left_input, right_input), joined.accumulate_output()))
        })?;

    let mut left_data = ([
        vec![Tup2(
            Tup2(1_usize, 1_usize),
            Tup2(Tup2(1_usize, 1_usize), 1_i64),
        )],
        vec![Tup2(Tup2(1, 1), Tup2(Tup2(1, 1), -3))],
        vec![Tup2(Tup2(1, 1), Tup2(Tup2(1, 1), 1))],
        vec![Tup2(Tup2(1, 1), Tup2(Tup2(1, 1), 1))],
        vec![Tup2(Tup2(1, 1), Tup2(Tup2(1, 1), 1))],
        vec![Tup2(Tup2(1, 1), Tup2(Tup2(1, 1), 0))],
    ] as [_; STEPS])
        .into_iter();

    let mut right_data = ([
        vec![Tup2(
            Tup2(1_usize, 1_usize),
            Tup2(Tup2(2_usize, 2_usize), 1_i64),
        )],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    ] as [_; STEPS])
        .into_iter();

    let mut expected_outputs = ([
        indexed_zset! {Tup2<usize, usize> => Tup4<usize, usize, usize, usize>:
            Tup2(1, 1) => { Tup4(1, 1, 2, 2) => 1 },    // "Lifetime" Total: +1
        },
        indexed_zset! {Tup2<usize, usize> => Tup4<usize, usize, usize, usize>:
            Tup2(1, 1) => { Tup4(1, 1, 2, 2) => -3 },   // "Lifetime" Total: -2
        },
        indexed_zset! {Tup2<usize, usize> => Tup4<usize, usize, usize, usize>:
            Tup2(1, 1) => { Tup4(1, 1, 2, 2) => 1 },    // "Lifetime" Total: -1
        },
        indexed_zset! {Tup2<usize, usize> => Tup4<usize, usize, usize, usize>:
            Tup2(1, 1) => { Tup4(1, 1, 2, 2) => 1 },    // "Lifetime" Total: +0
        },
        indexed_zset! {Tup2<usize, usize> => Tup4<usize, usize, usize, usize>:
            Tup2(1, 1) => { Tup4(1, 1, 2, 2) => 1 },    // "Lifetime" Total: +1
        },
        indexed_zset! {Tup2<usize, usize> => Tup4<usize, usize, usize, usize>:
            // No output delta because the zweight is 0; "Lifetime" Total: +1
        },
    ] as [_; STEPS])
        .into_iter();

    for i in (0..STEPS).confirm_each_auto().progress().with_bound() {
        left_input.append(&mut left_data.next().unwrap());
        right_input.append(&mut right_data.next().unwrap());
        circuit.transaction()?;
        let output = output.concat();
        print!("{}", output.iter().to_cli_table());
        assert_eq!(output.consolidate(), expected_outputs.next().unwrap());
    }

    Ok(())
}

#[test]
fn multiple_outputs() -> Result<(), anyhow::Error> {
    const STEPS: usize = 2;

    let (mut circuit, ((left_input, right_input), (filter_output, join_output))) =
        Runtime::init_circuit(worker_threads(), |root_circuit| {
            let (left, left_input) =
                root_circuit.add_input_indexed_zset::<Tup2<usize, usize>, Tup2<usize, usize>>();

            let (right, right_input) =
                root_circuit.add_input_indexed_zset::<Tup2<usize, usize>, Tup2<usize, usize>>();

            let left_filtered = left.filter(|(k, v)| k.0 == 2);

            let joined = left_filtered.join_index(&right, |k, Tup2(l1, l2), Tup2(r1, r2)| {
                // Merge left and right tuples.
                Some((*k, Tup4(*l1, *l2, *r1, *r2)))
            });

            // We output both the intermediate filter result and the final join result.
            Ok((
                (left_input, right_input),
                (
                    left_filtered.accumulate_output(),
                    joined.accumulate_output(),
                ),
            ))
        })?;

    let mut left_data = ([
        vec![
            Tup2(Tup2(1_usize, 1_usize), Tup2(Tup2(1_usize, 1_usize), 1_i64)),
            Tup2(Tup2(2, 1), Tup2(Tup2(1, 1), 1)),
            Tup2(Tup2(2, 3), Tup2(Tup2(1, 1), 1)),
        ],
        vec![
            Tup2(Tup2(1, 2), Tup2(Tup2(1, 1), 1)),
            Tup2(Tup2(2, 2), Tup2(Tup2(1, 1), 1)),
        ],
    ] as [_; STEPS])
        .into_iter();

    let mut right_data = ([
        vec![
            Tup2(Tup2(2_usize, 1_usize), Tup2(Tup2(2_usize, 2_usize), 1_i64)),
            Tup2(Tup2(2, 2), Tup2(Tup2(2, 2), 1)),
        ],
        vec![Tup2(Tup2(2, 3), Tup2(Tup2(2, 2), 1))],
    ] as [_; STEPS])
        .into_iter();

    let mut expected_filter_outputs = ([
        indexed_zset! {Tup2<usize, usize> => Tup2<usize, usize>:
            Tup2(2, 1) => { Tup2(1, 1) => 1 },
            Tup2(2, 3) => { Tup2(1, 1) => 1 },
        },
        indexed_zset! {Tup2<usize, usize> => Tup2<usize, usize>:
            Tup2(2, 2) => { Tup2(1, 1) => 1 },
        },
    ] as [_; STEPS])
        .into_iter();

    let mut expected_join_outputs = ([
        indexed_zset! {Tup2<usize, usize> => Tup4<usize, usize, usize, usize>:
            Tup2(2, 1) => { Tup4(1, 1, 2, 2) => 1 },
        },
        indexed_zset! {Tup2<usize, usize> => Tup4<usize, usize, usize, usize>:
            Tup2(2, 2) => { Tup4(1, 1, 2, 2) => 1 },
            Tup2(2, 3) => { Tup4(1, 1, 2, 2) => 1 },
        },
    ] as [_; STEPS])
        .into_iter();

    for i in (0..STEPS).confirm_each_auto().progress().with_bound() {
        left_input.append(&mut left_data.next().unwrap());
        right_input.append(&mut right_data.next().unwrap());
        circuit.transaction()?;
        let filter_output = filter_output.concat();
        print!("Filter\n{}", filter_output.iter().to_cli_table());
        assert_eq!(
            filter_output.consolidate(),
            expected_filter_outputs.next().unwrap()
        );
        let join_output = join_output.concat();
        print!("Join\n{}", join_output.iter().to_cli_table());
        assert_eq!(
            join_output.consolidate(),
            expected_join_outputs.next().unwrap()
        );
    }

    Ok(())
}

#[test]
fn rollback_test() -> Result<(), anyhow::Error> {
    const STEPS: usize = 4;

    let (mut circuit, ((left_input, right_input), output)) =
        Runtime::init_circuit(worker_threads(), |root_circuit| {
            let (left, left_input) =
                root_circuit.add_input_indexed_zset::<Tup2<usize, usize>, Tup2<usize, usize>>();

            let (right, right_input) =
                root_circuit.add_input_indexed_zset::<Tup2<usize, usize>, Tup2<usize, usize>>();

            let antijoined = left.antijoin(&right);

            Ok(((left_input, right_input), antijoined.accumulate_output()))
        })?;

    let mut left_data = ([
        // The initial transaction.
        vec![Tup2(
            Tup2(1_usize, 1_usize),
            // Note: Only the key matters for determining equality.
            Tup2(Tup2(1_usize, 1_usize), 1_i64),
        )],
        // A transaction which causes a constraint violation.
        vec![Tup2(Tup2(1, 2), Tup2(Tup2(1, 2), 1))],
        // We rollback/undo the previous transaction (by inverting the zweight).
        vec![Tup2(Tup2(1, 2), Tup2(Tup2(1, 2), -1))],
        // A new transaction which does not violate any constraint.
        vec![Tup2(Tup2(1, 2), Tup2(Tup2(1, 2), 1))],
    ] as [_; STEPS])
        .into_iter();

    let mut right_data = ([
        // The initial transaction.
        vec![Tup2(
            Tup2(1_usize, 1_usize),
            // Note: Only the key matters for determining equality.
            Tup2(Tup2(2_usize, 2_usize), 1_i64),
        )],
        // A transaction which causes a constraint violation.
        vec![],
        // We rollback/undo the previous transaction (by inverting the zweight).
        vec![],
        // A new transaction which does not violate any constraint.
        vec![
            Tup2(Tup2(1, 2), Tup2(Tup2(1, 2), 1)),
            // For L ANTIJOIN R it is okay to have R carry "excess" tuples.
            Tup2(Tup2(1, 3), Tup2(Tup2(1, 3), 1)),
        ],
    ] as [_; STEPS])
        .into_iter();

    let mut expected_outputs = ([
        indexed_zset! {Tup2<usize, usize> => Tup2<usize, usize>:
            // Empty output, i.e., no constraint violated.
        },
        indexed_zset! {Tup2<usize, usize> => Tup2<usize, usize>:
            // Violation due to non-empty result.
            Tup2(1, 2) => { Tup2(1, 2) => 1 },
        },
        indexed_zset! {Tup2<usize, usize> => Tup2<usize, usize>:
            // Rollback cleans operator state, result does not matter.
            Tup2(1, 2) => { Tup2(1, 2) => -1 },
        },
        indexed_zset! {Tup2<usize, usize> => Tup2<usize, usize>:
            // Empty output, i.e., no constraint violated.
        },
    ] as [_; STEPS])
        .into_iter();

    for i in (0..STEPS).confirm_each_auto().progress().with_bound() {
        left_input.append(&mut left_data.next().unwrap());
        right_input.append(&mut right_data.next().unwrap());
        circuit.transaction()?;
        let output = output.concat();
        print!("{}", output.iter().to_cli_table());
        assert_eq!(output.consolidate(), expected_outputs.next().unwrap());
    }

    Ok(())
}

// Computes the factorial of the first 10 numbers. This may be useful for custom
// termination criteria besides reaching a fixed point but we don't know yet.
#[test]
fn test_factorial_with_iterate() -> Result<(), anyhow::Error> {
    let (mut circuit, output) = Runtime::init_circuit(worker_threads(), |circuit| {
        // Generate sequence 0, 1, 2, ...
        let mut n: usize = 0;
        let source = circuit.add_source(Generator::new(move || {
            let result = n;
            n += 1;
            result
        }));
        // Compute factorial of each number in the sequence.
        let fact = circuit.iterate(|child_circuit| {
            let counter = Rc::new(RefCell::new(1));
            let counter_clone = Rc::clone(&counter);
            let countdown = source.delta0(child_circuit).apply(move |parent_val| {
                let mut counter_borrow = counter_clone.borrow_mut();
                *counter_borrow += *parent_val;
                let res = *counter_borrow;
                *counter_borrow -= 1;
                res
            });
            let (z1_output, z1_feedback) = child_circuit.add_feedback_with_export(Z1::new(1));
            let multiplication =
                countdown.apply2(&z1_output.local, |n1: &usize, n2: &usize| n1 * n2);
            z1_feedback.connect(&multiplication);
            // Stop iterating when the counter reaches 0.
            Ok((async move || Ok(*counter.borrow() == 0), z1_output.export))
        })?;
        Ok(fact.output())
    })?;

    let factorial = |n: usize| (1..=n).product::<usize>();
    const ITERATIONS: usize = 10;
    for i in 0..ITERATIONS {
        circuit.transaction()?;
        let result = output.take_from_all();
        let result = result.first().unwrap();
        println!("Iteration {:3}: {:3}! = {}", i + 1, i, result);
        assert_eq!(*result, factorial(i));
    }

    Ok(())
}

type NodeId = u64;
type Weight = u64;
type CumWeight = u64;
type Hopcnt = u64;
type WeightedEdge = Tup3<NodeId, NodeId, Weight>;
type Path = Tup4<NodeId, NodeId, CumWeight, Hopcnt>;

#[allow(clippy::type_complexity)]
fn trans_closure_data() -> (
    Vec<Vec<Tup2<WeightedEdge, ZWeight>>>,
    Vec<OrdZSet<Path>>,
) {
    let edges_input = vec![
        // The first clock cycle adds a graph of four nodes:
        // |0| -1-> |1| -1-> |2| -2-> |3| -2-> |4|
        vec![
            Tup2(Tup3(0, 1, 1), 1),
            Tup2(Tup3(1, 2, 1), 1),
            Tup2(Tup3(2, 3, 2), 1),
            Tup2(Tup3(3, 4, 2), 1),
        ],
        // The second clock cycle removes the edge |1| -1-> |2|.
        vec![Tup2(Tup3(1, 2, 1), -1)],
        // The third clock cycle would introduce a cycle but that would
        // cause the fixed point computation to never terminate.
        // In total, we have the following graph:
        // |0| -1-> |1| -1-> |2| -2-> |3| -2-> |4|
        //  ^                                   |
        //  |                                   |
        //  ------------------3------------------
        // zset_set! { Tup3(1,2,1), Tup3(4, 0, 3)}
    ];
    let expected_output = vec![
        zset! {
            Tup4(0, 1, 1, 1) => 1,
            Tup4(0, 2, 2, 2) => 1,
            Tup4(0, 3, 4, 3) => 1,
            Tup4(0, 4, 6, 4) => 1,
            Tup4(1, 2, 1, 1) => 1,
            Tup4(1, 3, 3, 2) => 1,
            Tup4(1, 4, 5, 3) => 1,
            Tup4(2, 3, 2, 1) => 1,
            Tup4(2, 4, 4, 2) => 1,
            Tup4(3, 4, 2, 1) => 1,
        },
        // These paths are removed in the second clock cycle.
        zset! {
            Tup4(0, 2, 2, 2) => -1,
            Tup4(0, 3, 4, 3) => -1,
            Tup4(0, 4, 6, 4) => -1,
            Tup4(1, 2, 1, 1) => -1,
            Tup4(1, 3, 3, 2) => -1,
            Tup4(1, 4, 5, 3) => -1,
        },
    ];
    (edges_input, expected_output)
}

/// Note that this example only works with acyclic graphs.
#[test]
fn test_self_rec_trans_closure_recursive() -> Result<(), anyhow::Error> {
    const STEPS: usize = 2;

    let (mut circuit_handle, (edges_input, output_handle)) =
        Runtime::init_circuit(worker_threads(), move |root_circuit| {
            let (edges, edges_input) = root_circuit.add_input_zset();

            // Create a base relation with all paths of length 1.
            let len_1 = edges.map(|Tup3(from, to, weight)| Tup4(*from, *to, *weight, 1));

            let closure = root_circuit.recursive(
            |child_circuit, len_n_minus_1: Stream<_, OrdZSet<Path>>| {
                // Import the `edges` and `len_1` relation from the parent circuit.
                let edges = edges.delta0(child_circuit);
                let len_1 = len_1.delta0(child_circuit);

                // Perform an iterative step (n-1 to n) through joining the
                // paths of length n-1 with the edges.
                let len_n = len_n_minus_1
                    .map_index(|Tup4(start, end, cum_weight, hopcnt)| {
                        (
                            *end,
                            Tup4(*start, *end, *cum_weight, *hopcnt),
                        )
                    })
                    .join(
                        &edges.map_index(|Tup3(from, to, weight)| {
                            (*from, Tup3(*from, *to, *weight))
                        }),
                        |_end_from,
                         Tup4(start, _end, cum_weight, hopcnt),
                         Tup3(_from, to, weight)| {
                            Tup4(*start, *to, cum_weight + weight, hopcnt + 1)
                        },
                    ).plus(&len_1);

                Ok(len_n)
            },
        )?;

            Ok((edges_input, closure.accumulate_output()))
        })?;

    let (edges_data, expected_outputs) = trans_closure_data();
    let mut edges_data = edges_data.into_iter();
    let mut expected_outputs = expected_outputs.into_iter();

    for _ in 0..STEPS {
        edges_input.append(&mut edges_data.next().unwrap());
        circuit_handle.transaction()?;
        let output = output_handle.concat().consolidate();
        assert_eq!(output, expected_outputs.next().unwrap());
    }

    Ok(())
}

/// This is a variant of [`test_self_rec_trans_closure_recursive`] using the iterate_with_condition() method.
// TODO and open questions:
// - Why does this not work for more than one thread? I suspect
//   this is due to the missing Consensus among worker threads, which
//   recursive has built in via some fixedpoint() method. A quick try in my
//   DBSP folk reveals it helps but it's still subject to race conditions.
//   Currently, investigating this with the DBSP folks.
#[test]
fn test_self_rec_trans_closure_iterate() -> Result<(), anyhow::Error> {
    const STEPS: usize = 2;

    let (mut circuit_handle, (edges_input, output_handle)) =
        Runtime::init_circuit(1, move |root_circuit| {
            let (edges, edges_input) = root_circuit.add_input_zset();

            // Create a base relation with all paths of length 1.
            let len_1 = edges.map(|Tup3(from, to, weight)| Tup4(*from, *to, *weight, 1));

            // Maximum recursion/iteration depth.
            const MAX_ITERATIONS: usize = 128;
            let iteration_count = Rc::new(RefCell::new(0));

            let closure = root_circuit.iterate_with_condition(|child_circuit| {
                // Feedback carries only the frontier (the delta from the last step).
                let (frontier, frontier_feedback) = child_circuit
                    .add_feedback(Z1::new(
                        OrdZSet::<Path>::default(),
                    ));

                // delta0 fires only at inner step 0, injecting the base case exactly once.
                let edges_inner = edges.delta0(child_circuit);
                let len_1_inner = len_1.delta0(child_circuit);

                // Extend the frontier by one hop.
                // At step 0: frontier={}, so the result is just len_1_inner, the base case.
                // At step n with n > 0: len_1_inner={}, so it's purely frontier ⋈ edges.
                let new_frontier = len_1_inner.plus(
                    &frontier
                        .map_index(move |Tup4(start, end, cum_weight, hopcnt)| {
                            (*end, Tup4(*start, *end, *cum_weight, *hopcnt))
                        })
                        .join(
                            &edges_inner.map_index(|Tup3(from, to, weight)| {
                                (*from, Tup3(*from, *to, *weight))
                            }),
                            |_end_from,
                             Tup4(start, _end, cum_weight, hopcnt),
                             Tup3(_from, to, weight)| {
                                Tup4(*start, *to, cum_weight + weight, hopcnt + 1)
                            },
                        ),
                );

                frontier_feedback.connect(&new_frontier);

                let condition = new_frontier.condition(move |z| {
                    let mut iteration_count = iteration_count.borrow_mut();
                    *iteration_count += 1;
                    let hit_max_iterations = *iteration_count >= MAX_ITERATIONS;
                    let no_more_delta = z.is_empty();
                    if no_more_delta {
                        *iteration_count = 0;
                    }
                    no_more_delta || hit_max_iterations
                });

                // Integrate across all inner iterations to collect every frontier.
                // The frontier at convergence is empty — we need the union of all of them.
                let all_paths = new_frontier.integrate();

                Ok((condition, all_paths.export()))
            })?;

            Ok((edges_input, closure.accumulate_output()))
        })?;

    let (edges_data, expected_output) = trans_closure_data();
    let mut edges_data = edges_data.into_iter();
    let mut expected_output = expected_output.into_iter();

    for i in (0..STEPS).confirm_each_auto().progress().with_bound() {
        println!("====== Inputs ======");
        let mut input = edges_data.next().unwrap();
        println!("Edges\n{}", input.iter().to_cli_table());
        edges_input.append(&mut input);

        circuit_handle.transaction()?;

        println!("====== Outputs ======");
        let output = output_handle.concat();
        println!("Transitive Closure\n{}", output.iter().to_cli_table());
        assert_eq!(output.consolidate(), expected_output.next().unwrap(),);
    }

    Ok(())
}

type Edge = Tup2<usize, usize>;
type Node = usize;

#[allow(clippy::type_complexity)]
fn graph_color_data() -> (
    Vec<Vec<Tup2<Node, ZWeight>>>,
    Vec<Vec<Tup2<Edge, ZWeight>>>,
    Vec<OrdZSet<Node>>,
    Vec<OrdZSet<Node>>,
) {
    let init_data = vec![vec![Tup2(0, 1)], vec![], vec![]];

    let edges_data = vec![
        // The first step adds a graph of four nodes:
        // |0| --> |1| --> |2| --> |3| --> |4|
        vec![
            Tup2(Tup2(0, 1), 1),
            Tup2(Tup2(1, 2), 1),
            Tup2(Tup2(2, 3), 1),
            Tup2(Tup2(3, 4), 1),
        ],
        // Now, we have the following graph in total:
        // |0| --> |1| --> |2| --> |3| --> |4|
        //  ^               |
        //  |               |
        //  ------ |5| <-----
        vec![Tup2(Tup2(2, 5), 1), Tup2(Tup2(5, 0), 1)],
        // And we introduce an odd-length cycle, rendering the graph
        // non-biparite anymore (all nodes are red _and_ blue):
        // |0| --> |1| --> |2| --> |3| --> |4|
        //  ^               |               |
        //  |               |               |
        //  ------ |5| <-----               |
        //  |                               |
        //  ---------------------------------
        vec![Tup2(Tup2(4, 0), 1)],
    ];

    let expected_red_output = vec![
        zset! {
            0 => 1,
            2 => 1,
            4 => 1,
        },
        zset! {},
        zset! {
            1 => 1,
            3 => 1,
            5 => 1,
        },
    ];

    let expected_blue_output = vec![
        zset! {
            1 => 1,
            3 => 1,
        },
        zset! {
            5 => 1,
        },
        zset! {
            0 => 1,
            2 => 1,
            4 => 1,
        },
    ];

    (
        init_data,
        edges_data,
        expected_red_output,
        expected_blue_output,
    )
}

/// This does mutual recursion of graph coloring using the recursive() method.
// TODO and open questions:
// - How to add a recursion/iteration depth limit with recursive()? I guess for
//   that the iterative API must be used..
// - Rewrite using `recursive_dynamic` once your PR got merged and released
//   https://github.com/feldera/feldera/pull/6577
#[test]
fn test_mutual_rec_graph_color_recursive() -> Result<(), anyhow::Error> {
    const STEPS: usize = 3;

    let (mut circuit_handle, ((init_input, edges_input), (red_output, blue_output))) =
        Runtime::init_circuit(worker_threads(), move |root_circuit| {
            let (edges, edges_input) = root_circuit.add_input_zset::<Edge>();
            let (init, init_input) = root_circuit.add_input_zset::<Node>();

            let (red, blue) = root_circuit.recursive(
                |child_circuit,
                 (red, blue): (
                    Stream<NestedCircuit, OrdZSet<Node>>,
                    Stream<NestedCircuit, OrdZSet<Node>>,
                )| {
                    // delta0 fires only at inner step 0, injecting the base case exactly once.
                    let edges = edges.delta0(child_circuit);
                    let init = init.delta0(child_circuit);

                    let new_red = blue
                        .map_index(|blue_node| (*blue_node, *blue_node))
                        .join(
                            &edges.map_index(|Tup2(from, to)| (*from, *to)),
                            |_blue_node, _, new_red_node| *new_red_node,
                        )
                        .plus(&init);

                    let new_blue = red.map_index(|red_node| (*red_node, *red_node)).join(
                        &edges.map_index(|Tup2(from, to)| (*from, *to)),
                        |_red_node, _, new_blue_node| *new_blue_node,
                    );

                    Ok((new_red, new_blue))
                },
            )?;

            Ok((
                (init_input, edges_input),
                (red.accumulate_output(), blue.accumulate_output()),
            ))
        })?;

    let (init_data, edges_data, expected_red_output, expected_blue_output) = graph_color_data();
    let mut init_data = init_data.into_iter();
    let mut edges_data = edges_data.into_iter();
    let mut expected_red_output = expected_red_output.into_iter();
    let mut expected_blue_output = expected_blue_output.into_iter();

    for i in (0..STEPS).confirm_each_auto().progress().with_bound() {
        println!("====== Inputs ======");
        let mut input = init_data.next().unwrap();
        println!("Init\n{}", input.iter().to_cli_table());
        init_input.append(&mut input);
        let mut input = edges_data.next().unwrap();
        println!("Edges\n{}", input.iter().to_cli_table());
        edges_input.append(&mut input);

        circuit_handle.transaction()?;

        println!("====== Outputs ======");
        let output = red_output.concat();
        println!("Red\n{}", output.iter().to_cli_table());
        assert_eq!(output.consolidate(), expected_red_output.next().unwrap(),);
        let output = blue_output.concat();
        println!("Blue\n{}", output.iter().to_cli_table());
        assert_eq!(output.consolidate(), expected_blue_output.next().unwrap(),);
    }

    Ok(())
}

/// This does mutual recursion using the iterate_with_conditions() method.
// TODO and open questions:
// - Why does this produce wrong results starting from the second iteration?
// - If the result is correct someday, does this work for more than one thread?
#[test]
#[should_panic] // As of now, this is the expected behavior due the TODOs above.
fn test_mutual_rec_graph_color_iterate() {
    const STEPS: usize = 3;

    let (mut circuit_handle, ((init_input, edges_input), (red_output, blue_output))) =
        Runtime::init_circuit(1, move |root_circuit| {
            let (edges, edges_input) = root_circuit.add_input_zset::<Edge>();
            let (init, init_input) = root_circuit.add_input_zset::<Node>();

            // Safety measure to prevent infinite iterations.
            const MAX_ITERATIONS: usize = 128;
            let iteration_count = Rc::new(RefCell::new(0));

            let mut outputs = root_circuit.iterate_with_conditions(|child_circuit| {
                // Feedback carries only the frontier (the delta from the last step).
                let (red, red_feedback) =
                    child_circuit.add_feedback(Z1::new(OrdZSet::<usize>::default()));
                let (blue, blue_feedback) =
                    child_circuit.add_feedback(Z1::new(OrdZSet::<usize>::default()));

                // delta0 fires only at inner step 0, injecting the base case exactly once.
                let edges = edges.delta0(child_circuit);
                let init = init.delta0(child_circuit);

                let new_red = blue
                    .map_index(|blue_node| (*blue_node, *blue_node))
                    .join(
                        &edges.map_index(|Tup2(from, to)| (*from, *to)),
                        |_blue_node, _, new_red_node| *new_red_node,
                    )
                    .plus(&init);

                new_red.inspect(|x| println!("RED DISCOVERED: {:?}", x));

                let new_blue = red.map_index(|red_node| (*red_node, *red_node)).join(
                    &edges.map_index(|Tup2(from, to)| (*from, *to)),
                    |_red_node, _, new_blue_node| *new_blue_node,
                );

                new_blue.inspect(|x| println!("BLUE DISCOVERED: {:?}", x));

                red_feedback.connect(&new_red);
                blue_feedback.connect(&new_blue);

                let red_done = new_red.condition(move |z| z.is_empty());
                let blue_done = new_blue.condition(move |z| z.is_empty());

                // Integrate across all inner iterations to collect every frontier.
                // The frontier at convergence is empty — we need the union of all of them.
                let all_red = new_red.integrate();
                let all_blue = new_blue.integrate();

                Ok((
                    vec![red_done, blue_done],
                    vec![Some(all_red.export()), Some(all_blue.export())],
                ))
            })?;
            let red = outputs[0].take().unwrap();
            let blue = outputs[1].take().unwrap();

            Ok((
                (init_input, edges_input),
                (red.accumulate_output(), blue.accumulate_output()),
            ))
        })
        .expect("Circuit builds");

    let (init_data, edges_data, expected_red_output, expected_blue_output) = graph_color_data();
    let mut init_data = init_data.into_iter();
    let mut edges_data = edges_data.into_iter();
    let mut expected_red_output = expected_red_output.into_iter();
    let mut expected_blue_output = expected_blue_output.into_iter();

    for i in (0..STEPS).confirm_each_auto().progress().with_bound() {
        println!("====== Inputs ======");
        let mut input = init_data.next().unwrap();
        println!("Init\n{}", input.iter().to_cli_table());
        init_input.append(&mut input);
        let mut input = edges_data.next().unwrap();
        println!("Edges\n{}", input.iter().to_cli_table());
        edges_input.append(&mut input);

        circuit_handle.transaction().expect("Transaction succeeds");

        println!("====== Outputs ======");
        let output = red_output.concat();
        println!("Red\n{}", output.iter().to_cli_table());
        assert_eq!(output.consolidate(), expected_red_output.next().unwrap(),);
        let output = blue_output.concat();
        println!("Blue\n{}", output.iter().to_cli_table());
        assert_eq!(output.consolidate(), expected_blue_output.next().unwrap(),);
    }
}

// Also notice the points_to_step_{1,2,3}.dl files for a Datalog implementation
// of this query which can be executed on Souffle.
#[test]
fn test_mutual_recursion() -> Result<(), anyhow::Error> {
    type String2 = Tup2<String, String>;
    type String3 = Tup3<String, String, String>;

    const STEPS: usize = 3;

    let (
        mut circuit,
        (
            (
                alloc_input,
                assign_input,
                virtual_call_input,
                heap_type_input,
                dispatch_input,
                actual_arg_input,
                formal_param_input,
            ),
            (var_points_to_output, call_graph_output),
        ),
    ) = Runtime::init_circuit(worker_threads(), move |root_circuit| {
        let (alloc, alloc_input) = root_circuit.add_input_zset::<String2>();
        let (assign, assign_input) = root_circuit.add_input_zset::<String2>();
        let (virtual_call, virtual_call_input) = root_circuit.add_input_zset::<String3>();
        let (heap_type, heap_type_input) = root_circuit.add_input_zset::<String2>();
        let (dispatch, dispatch_input) = root_circuit.add_input_zset::<String3>();
        let (actual_arg, actual_arg_input) = root_circuit.add_input_zset::<String2>();
        let (formal_param, formal_param_input) = root_circuit.add_input_zset::<String2>();

        let (var_points_to, call_graph) = root_circuit.recursive(
            |child_circuit,
             (var_points_to, call_graph): (
                Stream<_, OrdZSet<String2>>,
                Stream<_, OrdZSet<String2>>,
            )| {
                // Import streams from the parent circuit into the child circuit.
                let alloc = alloc.delta0(child_circuit);
                let assign = assign.delta0(child_circuit);
                let virtual_call = virtual_call.delta0(child_circuit);
                let heap_type = heap_type.delta0(child_circuit);
                let dispatch = dispatch.delta0(child_circuit);
                let actual_arg = actual_arg.delta0(child_circuit);
                let formal_param = formal_param.delta0(child_circuit);

                let call_graph_next = virtual_call
                    .map_index(|Tup3(site, recv, sig)| {
                        (recv.clone(), (site.clone(), recv.clone(), sig.clone()))
                    })
                    .join_index(
                        // 1. virtual_call JOIN var_points_to ON recv
                        &var_points_to.map_index(|Tup2(recv, obj)| {
                            (recv.clone(), (recv.clone(), obj.clone()))
                        }),
                        |_recv, (site, _, sig), (_, obj)| {
                            Some((obj.clone(), Tup3(site.clone(), sig.clone(), obj.clone())))
                        },
                    )
                    .join_index(
                        // 2. ... JOIN heap_type ON obj
                        &heap_type
                            .map_index(|Tup2(obj, ty)| (obj.clone(), (obj.clone(), ty.clone()))),
                        |_obj, Tup3(site, sig, _), (_, ty)| {
                            Some(((ty.clone(), sig.clone()), (site.clone(), ty.clone())))
                        },
                    )
                    .join_index(
                        // 3. ... JOIN dispatch ON ty and sig
                        &dispatch.map_index(|Tup3(ty, sig, meth)| {
                            ((ty.clone(), sig.clone()), meth.clone())
                        }),
                        |_, (site, _), meth| {
                            Some((
                                (site.clone(), meth.clone()),
                                Tup2(site.clone(), meth.clone()),
                            ))
                        },
                    );

                let var_points_to_next = var_points_to
                    .map_index(|Tup2(src, obj)| (src.clone(), (src.clone(), obj.clone())))
                    .join_index(
                        &assign
                            .map_index(|Tup2(dst, src)| (src.clone(), (dst.clone(), src.clone()))),
                        |_src, (_, obj), (dst, _)| {
                            Some(((dst.clone(), obj.clone()), (dst.clone(), obj.clone())))
                        },
                    )
                    .plus(&alloc.map_index(|Tup2(var, obj)| {
                        ((var.clone(), obj.clone()), (var.clone(), obj.clone()))
                    }))
                    .plus(
                        &call_graph
                            .map_index(|Tup2(site, meth)| {
                                (site.clone(), (site.clone(), meth.clone()))
                            })
                            .join_index(
                                // 1. call_graph JOIN actual_arg ON site
                                &actual_arg.map_index(|Tup2(site, arg)| {
                                    (site.clone(), (site.clone(), arg.clone()))
                                }),
                                |_site, (_, meth), (_, arg)| {
                                    Some((meth.clone(), (meth.clone(), arg.clone())))
                                },
                            )
                            .join_index(
                                // .2. ... JOIN formal_param ON meth
                                &formal_param.map_index(|Tup2(meth, param)| {
                                    (meth.clone(), (meth.clone(), param.clone()))
                                }),
                                |_meth, (_, arg), (_, param)| {
                                    Some(((arg.clone()), (arg.clone(), param.clone())))
                                },
                            )
                            .join_index(
                                // 3. ... JOIN var_points_to ON arg
                                &var_points_to.map_index(|Tup2(arg, obj)| {
                                    (arg.clone(), (arg.clone(), obj.clone()))
                                }),
                                |_arg, (_, param), (_, obj)| {
                                    Some((
                                        (param.clone(), obj.clone()),
                                        (param.clone(), obj.clone()),
                                    ))
                                },
                            ),
                    );

                Ok((
                    var_points_to_next.map(|(_, (param, obj))| Tup2(param.clone(), obj.clone())),
                    call_graph_next.map(|((site, meth), _)| Tup2(site.clone(), meth.clone())),
                ))
            },
        )?;

        Ok((
            (
                alloc_input,
                assign_input,
                virtual_call_input,
                heap_type_input,
                dispatch_input,
                actual_arg_input,
                formal_param_input,
            ),
            (
                var_points_to.accumulate_output(),
                call_graph.accumulate_output(),
            ),
        ))
    })?;

    fn owned_string2(
        ((s1, s2), weight): ((&str, &str), ZWeight),
    ) -> Tup2<Tup2<String, String>, ZWeight> {
        Tup2(Tup2(s1.to_owned(), s2.to_owned()), weight)
    }
    fn owned_string3(
        ((s1, s2, s3), weight): ((&str, &str, &str), ZWeight),
    ) -> Tup2<Tup3<String, String, String>, ZWeight> {
        Tup2(Tup3(s1.to_owned(), s2.to_owned(), s3.to_owned()), weight)
    }

    let mut alloc_inputs = ([
        vec![(("g", "oG"), 1), (("d", "oDog"), 1), (("c", "oCat"), 1)]
            .into_iter()
            .map(owned_string2)
            .collect(),
        vec![(("m", "oMouse"), 1)]
            .into_iter()
            .map(owned_string2)
            .collect(),
        vec![],
    ] as [Batch<String2>; STEPS])
        .into_iter();

    let mut assign_inputs = ([
        vec![(("ac", "c"), 1)]
            .into_iter()
            .map(owned_string2)
            .collect(),
        vec![],
        vec![(("ac", "c"), -1)]
            .into_iter()
            .map(owned_string2)
            .collect(),
    ] as [Batch<String2>; STEPS])
        .into_iter();

    let mut virtual_call_inputs = ([
        vec![
            (("s1", "g", "greet"), 1),
            (("s2", "g", "greet"), 1),
            (("s3", "x", "speak"), 1),
        ]
        .into_iter()
        .map(owned_string3)
        .collect(),
        vec![(("s4", "g", "greet"), 1)]
            .into_iter()
            .map(owned_string3)
            .collect(),
        vec![(("s2", "g", "greet"), -1)]
            .into_iter()
            .map(owned_string3)
            .collect(),
    ] as [Batch<String3>; STEPS])
        .into_iter();

    let mut heap_type_inputs = ([
        vec![
            (("oG", "Greeter"), 1),
            (("oDog", "Dog"), 1),
            (("oCat", "Cat"), 1),
        ]
        .into_iter()
        .map(owned_string2)
        .collect(),
        vec![(("oMouse", "Mouse"), 1)]
            .into_iter()
            .map(owned_string2)
            .collect(),
        vec![],
    ] as [Batch<String2>; STEPS])
        .into_iter();

    let mut dispatch_inputs = ([
        vec![
            (("Greeter", "greet", "Greeter.greet"), 1),
            (("Dog", "speak", "Dog.speak"), 1),
            (("Cat", "speak", "Cat.speak"), 1),
        ]
        .into_iter()
        .map(owned_string3)
        .collect(),
        vec![(("Mouse", "speak", "Mouse.speak"), 1)]
            .into_iter()
            .map(owned_string3)
            .collect(),
        vec![],
    ] as [Batch<String3>; STEPS])
        .into_iter();

    let mut actual_arg_inputs = ([
        vec![(("s1", "d"), 1), (("s2", "ac"), 1)]
            .into_iter()
            .map(owned_string2)
            .collect(),
        vec![(("s4", "m"), 1)]
            .into_iter()
            .map(owned_string2)
            .collect(),
        vec![(("s2", "ac"), 1)]
            .into_iter()
            .map(owned_string2)
            .collect(),
    ] as [Batch<String2>; STEPS])
        .into_iter();

    let mut formal_param_inputs = ([
        vec![(("Greeter.greet", "x"), 1)]
            .into_iter()
            .map(owned_string2)
            .collect(),
        vec![],
        vec![],
    ] as [Batch<String2>; STEPS])
        .into_iter();

    let mut var_points_to_expected_outputs = ([
        zset! {
            Tup2("ac".to_string(), "oCat".to_string()) => 1,
            Tup2("c".to_string(), "oCat".to_string()) => 1,
            Tup2("d".to_string(), "oDog".to_string()) => 1,
            Tup2("g".to_string(), "oG".to_string()) => 1,
            Tup2("x".to_string(), "oDog".to_string()) => 1,
            Tup2("x".to_string(), "oCat".to_string()) => 1,
        },
        zset! {
            Tup2("m".to_string(), "oMouse".to_string()) => 1,
            Tup2("x".to_string(), "oMouse".to_string()) => 1,
        },
        zset! {
            Tup2("ac".to_string(), "oCat".to_string()) => -1,
            Tup2("x".to_string(), "oCat".to_string()) => -1,
        },
    ] as [OrdZSet<String2>; STEPS])
        .into_iter();

    let mut call_graph_expected_outputs = ([
        zset! {
            Tup2("s1".to_string(), "Greeter.greet".to_string()) => 1,
            Tup2("s2".to_string(), "Greeter.greet".to_string()) => 1,
            Tup2("s3".to_string(), "Dog.speak".to_string()) => 1,
            Tup2("s3".to_string(), "Cat.speak".to_string()) => 1,
        },
        zset! {
            Tup2("s3".to_string(), "Mouse.speak".to_string()) => 1,
            Tup2("s4".to_string(), "Greeter.greet".to_string()) => 1,
        },
        zset! {
            Tup2("s2".to_string(), "Greeter.greet".to_string()) => -1,
            Tup2("s3".to_string(), "Cat.speak".to_string()) => -1,
        },
    ] as [OrdZSet<String2>; STEPS])
        .into_iter();

    for i in (0..STEPS).confirm_each_auto().progress().with_bound() {
        println!("====== Inputs ======");

        let mut input = alloc_inputs.next().unwrap();
        println!("Alloc\n{}", input.iter().to_cli_table());
        alloc_input.append(&mut input);

        let mut input = assign_inputs.next().unwrap();
        println!("Assign\n{}", input.iter().to_cli_table());
        assign_input.append(&mut input);

        let mut input = virtual_call_inputs.next().unwrap();
        println!("VirtualCall\n{}", input.iter().to_cli_table());
        virtual_call_input.append(&mut input);

        let mut input = heap_type_inputs.next().unwrap();
        println!("HeapType\n{}", input.iter().to_cli_table());
        heap_type_input.append(&mut input);

        let mut input = dispatch_inputs.next().unwrap();
        println!("Dispatch\n{}", input.iter().to_cli_table());
        dispatch_input.append(&mut input);

        let mut input = actual_arg_inputs.next().unwrap();
        println!("ActualArg\n{}", input.iter().to_cli_table());
        actual_arg_input.append(&mut input);

        let mut input = formal_param_inputs.next().unwrap();
        println!("FormalParam\n{}", input.iter().to_cli_table());
        formal_param_input.append(&mut input);

        circuit.transaction()?;

        println!("====== Outputs ======");
        let var_points_to_output = var_points_to_output.concat();
        println!(
            "VarPointsTo\n{}",
            var_points_to_output.iter().to_cli_table()
        );
        assert_eq!(
            var_points_to_output.consolidate(),
            var_points_to_expected_outputs.next().unwrap(),
        );
        let call_graph_output = call_graph_output.concat();
        println!("CallGraph\n{}", call_graph_output.iter().to_cli_table());
        assert_eq!(
            call_graph_output.consolidate(),
            call_graph_expected_outputs.next().unwrap(),
        );
    }

    Ok(())
}
