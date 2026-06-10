import * as runtime from './runtime.ts';
import { Graph, GraphOfGraphs } from './graph.ts';

function assert(b: boolean) {
  if (!b) {
    throw 'assertion failed'
  }
}

function graph_tests() {
  const g = Graph.create();

  const v0 = g.vertex.add();
  const v1 = g.vertex.add();

  assert (g.vertex.has(v1));

  const e0 = g.edge(v0)(v1).add();

  assert (g.edge(v0)(v1).has(e0));
  assert (!g.edge(v1)(v1).has(e0));

  const e1 = g.edge(v1)(v1).add();

  assert (!g.edge(v0)(v1).has(e1));
  assert (!runtime.Value.equal(e0, e1));
  assert (runtime.Value.equal(e1, e1));
   
  console.log("graph tests passed");
}

graph_tests();

function graph_of_graph_tests() {
  const gg = GraphOfGraphs.create();

  const g0_id = gg.graphs.add();
  const g0 = gg.graph(g0_id);
  var v0 = g0.vertex.add();
  assert (g0.vertex.has(v0));
  var e0 = g0.edge(v0)(v0).add();
  assert (g0.edge(v0)(v0).has(e0));
  assert (gg.graph(g0_id).edge(v0)(v0).has(e0));
  
  const g1_id = gg.graphs.add();
  const g1 = gg.graph(g1_id);
  
  assert (!g1.vertex.has(v0));
  const v1 = g1.vertex.add();
  
  assert (!g1.edge(v1)(v1).has(e0));

  console.log("graph of graphs tests passed");
}

graph_of_graph_tests();
