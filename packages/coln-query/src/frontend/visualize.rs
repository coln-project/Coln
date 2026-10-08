// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An [`Analysis`] in GraphViz's DOT language: the quotient graph laid over
//! the predicate dependency graph it is the quotient of.
//!
//! - Every predicate is a node, labeled with its name, its columns, and how
//!   many rules define it.
//! - Every component is a cluster around its members, labeled at the bottom
//!   with its position in the execution order, and whether it is recursive. A
//!   component of a single predicate is a cluster all the same, so that every
//!   predicate's component is visible.
//! - Every dependency `p -> q`, read as "`p` depends on `q`", is an [`Edge`]
//!   styled by its polarity, and by whether it stays within a component.
//! - A component's border is a [`Node`] styled by whether the component is a
//!   source, a sink, or neither. One that is both a source and a sink is
//!   styled as a source, as a cluster has but one border.

use super::{
    Component, Identifiable, Predicate,
    analysis::{Analysis, ComponentView},
};
use std::{collections::HashMap, fmt};

/// See the [module docs](self).
pub struct Dot<'g, 'a, P: Identifiable> {
    analysis: &'g Analysis<'a, P>,
    columns: Columns,
}

/// Whether a predicate's node lists the predicate's columns below its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Columns {
    Shown,
    Hidden,
}

impl<'a, P: Predicate> Analysis<'a, P> {
    /// The analysis in GraphViz's DOT language, as for `dot -Tsvg`.
    pub fn dot(&self, columns: Columns) -> Dot<'_, 'a, P> {
        Dot {
            analysis: self,
            columns,
        }
    }
}

/// The font of every label, as a list of families tried in order, like CSS's
/// `font-family`. GraphViz does not pass a graph's font on to its nodes and
/// edges, so each of the three takes it.
const FONT: &str = "Helvetica, sans-serif";

/// Can be applied to components and nodes.
///
/// Only applies to borders, we don't fill nodes, ever.
struct Node {
    /// Border color.
    color: &'static str,
    /// Border style: solid, dashed, dotted.
    style: &'static str,
    /// Bold border.
    bold: bool,
    /// Rounded border.
    rounded: bool,
}

impl Node {
    /// A predicate node.
    const fn new_node() -> Self {
        Self {
            color: "black",
            style: "solid",
            bold: false,
            rounded: false,
        }
    }
    /// A regular (non-source, non-sink) component.
    const fn new_component() -> Self {
        Self {
            color: "black",
            style: "solid",
            bold: false,
            rounded: true,
        }
    }
    /// A source component.
    fn source(mut self) -> Self {
        self.color = "green";
        self.style = "dashed";
        self
    }
    /// A sink component.
    fn sink(mut self) -> Self {
        self.color = "blue";
        self.style = "dashed";
        self
    }
}

/// As DOT attributes, for the attribute list of a node or a cluster.
impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "color={}, style=\"{}", self.color, self.style)?;
        if self.bold {
            f.write_str(",bold")?;
        }
        if self.rounded {
            f.write_str(",rounded")?;
        }
        f.write_str("\"")
    }
}

struct Edge {
    label: &'static str,
    style: &'static str,
    color: &'static str,
}

impl Edge {
    const fn new() -> Self {
        Edge {
            label: "",
            style: "solid",
            color: "black",
        }
    }
    fn inter(mut self) -> Self {
        self.style = "dashed";
        self
    }
    fn positive(mut self) -> Self {
        self.label = "+";
        self
    }
    fn negative(mut self) -> Self {
        self.label = "¬";
        self.color = "red";
        self
    }
}

/// As DOT attributes, for the attribute list of an edge.
impl fmt::Display for Edge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Edge {
            label,
            style,
            color,
        } = self;
        write!(
            f,
            "label=\"{label}\", style={style}, color={color}, fontcolor={color}"
        )
    }
}

impl<P: Predicate> fmt::Display for Dot<'_, '_, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let analysis = self.analysis;
        // Node ids are minted rather than taken from the predicates' names,
        // which may contain anything. The names go into labels instead.
        let nodes: HashMap<&P::Identifier, usize> = analysis
            .execution_order()
            .iter()
            .flat_map(Component::members)
            .enumerate()
            .map(|(node, predicate)| (predicate.id(), node))
            .collect();

        writeln!(f, "digraph {{")?;
        writeln!(f, "  graph [fontname=\"{FONT}\"];")?;
        writeln!(
            f,
            "  node [shape=box, fontname=\"{FONT}\", {}];",
            Node::new_node()
        )?;
        writeln!(f, "  edge [fontname=\"{FONT}\"];")?;
        for component in analysis.components() {
            cluster(f, &component, &nodes, self.columns)?;
        }
        for edge in analysis.edges() {
            let style = match edge.intra {
                true => Edge::new(),
                false => Edge::new().inter(),
            };
            let style = match edge.negative {
                true => style.negative(),
                false => style.positive(),
            };
            writeln!(
                f,
                "  n{} -> n{} [{style}];",
                nodes[edge.from.id()],
                nodes[edge.to.id()],
            )?;
        }
        f.write_str("}")
    }
}

/// The component's cluster, around its members.
fn cluster<P: Predicate>(
    f: &mut fmt::Formatter<'_>,
    view: &ComponentView<'_, '_, P>,
    nodes: &HashMap<&P::Identifier, usize>,
    columns: Columns,
) -> fmt::Result {
    let position = view.position;
    let border = match (view.is_source, view.is_sink) {
        (true, _) => Node::new_component().source(),
        (false, true) => Node::new_component().sink(),
        (false, false) => Node::new_component(),
    };
    let recursive = if view.component.is_recursive() {
        " (recursive)"
    } else {
        ""
    };
    writeln!(f, "  subgraph cluster_{position} {{")?;
    writeln!(
        f,
        "    graph [label=\"c{position}{recursive}\", labelloc=b, {border}];"
    )?;
    for member in view.component.members() {
        writeln!(
            f,
            "    n{} [label={}];",
            nodes[member.id()],
            PredicateLabel(member, columns)
        )?;
    }
    writeln!(f, "  }}")
}

/// A predicate's node label, as in an ER diagram: its name in bold, then if
/// [`Columns::Shown`], its columns one per line, and finally how many rules
/// define it, each section below a horizontal line.
///
/// An EDB predicate shows no rule count: its single rule only marks it as
/// given externally, which is why the Datalog notation does not print it
/// either.
///
/// An HTML-like label, as only those draw a line. The node's own border stays
/// the box around it, styled by [`Node`].
struct PredicateLabel<'p, P>(&'p P, Columns);

impl<P: Predicate> fmt::Display for PredicateLabel<'_, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let PredicateLabel(predicate, shown) = self;
        write!(
            f,
            "<<TABLE BORDER=\"0\" CELLSPACING=\"0\"><TR><TD><B>{}</B></TD></TR>",
            escaped(&predicate.id().to_string()),
        )?;
        if *shown == Columns::Shown {
            let mut columns = predicate.columns().peekable();
            // A line may only stand between two rows, so none for no columns.
            if columns.peek().is_some() {
                f.write_str("<HR/>")?;
            }
            columns.try_for_each(|column| {
                write!(
                    f,
                    "<TR><TD ALIGN=\"LEFT\">{}</TD></TR>",
                    escaped(&column.to_string())
                )
            })?;
        }
        if predicate.is_idb_predicate() {
            let rules = predicate.rules().count();
            let noun = if rules == 1 { "rule" } else { "rules" };
            write!(f, "<HR/><TR><TD ALIGN=\"LEFT\">{rules} {noun}</TD></TR>")?;
        }
        f.write_str("</TABLE>>")
    }
}

/// `text` as the content of an HTML-like label.
fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::super::{LogicalProgram, test_utils::*};
    use super::*;

    #[test]
    fn components_edges_sources_and_sinks_are_told_apart() {
        // `edge` is a source, `unreached` a sink, and `path` neither. `path`
        // reads `edge` in both of its rules, which is one edge nonetheless.
        let program = program(vec![
            pred("edge", &[&[]]),
            pred("path", &[&["edge"], &["path", "edge"]]),
            pred("unreached", &[&["path", "!edge"]]),
        ]);
        let analysis = program.verify().expect("the program is stratifiable");
        let dot = analysis.dot(Columns::Shown).to_string();

        let cluster = |label, border| format!("graph [label=\"{label}\", labelloc=b, {border}];");
        let edge = |from, to, style| format!("n{from} -> n{to} [{style}];");
        let expected = [
            cluster("c0", Node::new_component().source()),
            cluster("c1 (recursive)", Node::new_component()),
            cluster("c2", Node::new_component().sink()),
            // A predicate's name, its columns, and its rule count, each
            // section below a line.
            "<B>path</B></TD></TR><HR/><TR><TD ALIGN=\"LEFT\">x: uint</TD></TR>\
             <HR/><TR><TD ALIGN=\"LEFT\">2 rules</TD></TR>"
                .to_string(),
            // An EDB predicate shows no rule count.
            "<B>edge</B></TD></TR><HR/><TR><TD ALIGN=\"LEFT\">x: uint</TD></TR></TABLE>"
                .to_string(),
            // Within a component, between components, and negative.
            edge(1, 1, Edge::new().positive()),
            edge(1, 0, Edge::new().inter().positive()),
            edge(2, 0, Edge::new().inter().negative()),
        ];
        for line in &expected {
            assert!(dot.contains(line), "missing {line:?} in:\n{dot}");
        }
        assert_eq!(dot.matches("n1 -> n0").count(), 1, "{dot}");

        // Hidden, a predicate's node keeps its name and rule count only.
        let dot = analysis.dot(Columns::Hidden).to_string();
        assert!(
            dot.contains(
                "<TR><TD><B>path</B></TD></TR><HR/><TR><TD ALIGN=\"LEFT\">2 rules</TD></TR></TABLE>"
            ),
            "{dot}"
        );
        assert!(!dot.contains("x: uint"), "{dot}");
    }
}
