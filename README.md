# Collaborative Geolog

> [!IMPORTANT]
> This codebase is _very very vibecoded_, do not consider the code to be 
> authoritative in any way. 

This project is an experiment to help think about how Geolog should work in a
collaborative environment. The main purpose is to have a concrete, end-to-end
demo of Geolog which we can point at and say "not like that". I.e. it provides
a jumping off point for discussion about how Geolog should work in
collaborative environments. As such expect lots of things to be wrong, poorly
conceived, completely missing, etc.

Concretely, this project is two things:

* A rust library implementing a parser for the
  [geolog-zeta](https://git.sgai.uk/davidad/geolog-zeta) syntax. As well as a
  commit graph representation of the history of a database
* A web application which uses the above library (compiled to webassembly) to
  implement a collaborative database by storing the commit graph in Automerge

## Running It

You'll need a few things:

* A rust toolchain with the `wasm32-unknown-unknown` target installed
* [`wasm-bindgen-cli`](https://github.com/wasm-bindgen/wasm-bindgen)
* [`wasm-bodge`](https://github.com/alexjg/wasm-bodge)
* `pnpm`

If you have all of these then you need to do the following:

First, build the WebAssembly library

```bash
cd ./geolog-rs/crates/geolog-wasm && npm run build
```

Now, install the dependencies of the web app

```bash
cd ./geolog-web && pnpm install
```

Now run the dev server:

```bash
cd ./geolog-web && pnpm run dev
```

You should now be able to open `http://localhost:5173` and see the app running.
The web app starts by asking you to input a theory - with an example already
filled in - and then once you click "Create Database" it will switch to a
view of two databases with a "connect" button between them. You can use this
to experiment with what happens when you modify the database concurrently.

## What is it though?

The primary goal of this experiment is to help think about what it means to
have concurrent edits to an instance of a Geolog theory when that theory has
axioms that might be violated by concurrent edits. Geolog is a little nebulous
right now, so I'll try and be specific about what that means to me.

I mostly think of Geolog as a database. This project uses syntax from
[geolog-zeta](https://git.sgai.uk/davidad/geolog-zeta) to define theories,
which look like this:

```
theory Graph {
  Vertex : Sort;
  Edge : [src: Vertex, tgt: Vertex] -> Prop;
}
```

I think of theories as being a sort of entity-relationship schema. `Sort`s are
like entities - they are things which have a unique identity and a type name
and nothing else. `Prop`s are like relationships - they are things which relate
together a bunch of entities and values.

In this example then we have a collection of "Vertex" things, which have their
own identity, and an "Edge" relationship between them, where for any two
vertices there either is or isn't an edge between them.

### Operations

In order to be able to persist and synchronise a database we need some concrete
representation of it. Given the entity and relation view of theories above, a
simple way to represent the state of a database is as a set of operations which
create entities and relations. Schematically operations look a little like:

```typescript
type Operation = 
  | {type: "addEntity", entityType: string, entityId: string}
  | {type: "addRelation", relationType: string, args: Array<{entityId: string}>}
```

What this means is that each entity is identified by a unique string `entityId`
(we use UUIDs) and each relation is identified by the arguments it relates
together (i.e. multiple addRelation ops with the same contents are idemptotent).

For example, here's how we might represent the graph with two vertices
`A` and `B` and an edge from `A` to `B`:

```js
[
  {type: "addEntity", entityType: "Vertex", entityId: "1234"}, // A
  {type: "addEntity", entityType: "Vertex", entityId: "5678"}, // B
  {type: "addRelation", relationType: "Edge", args: [
    {entityId: "1234"},
    {entityId: "5678"}
  ]} // Edge from A to B
]
```

Now in order to figure out what the current state of the database is we
evaluate all of the operations.

### Axioms and Concurrent Violations Thereof

The graph above is kind of boring. Here's a more interesting one:

```
theory WeightedGraph {
  Vertex : Sort;
  Edge : [src: Vertex, tgt: Vertex, weight: Int] -> Prop;
  
  ax/unique_weight : forall v1 : Vertex, v2 : Vertex.
    [src: v1, tgt: v2, weight: n1] Edge /\ [src: v1, tgt: v2, weight: n2] Edge
    |- n1 = n2;
}
```

This graph says that every edge has a weight. But more interestingly, it has
an axiom which must be true of any view of the database which says that the
weight of an edge between two vertices must be unique. I.e. you can't have two
edges between the same two vertices with different weights.

The reason this is interesting is because whilst it's easy to maintain this
invariant locally, you can't maintain it globally in the face of concurrent
edits. For example, if I add an edge from A to B with weight 1, and at the same
time you add an edge from A to B with weight 2 then we both have a locally
valid view of the database, but if we merge our changes together then we have
an instance of the database which violates the axiom.

In order to resolve this we do something naive. Rather than storing the
operations as a log, we store them as a commit graph. Each operation refers
to its parents (by hash) and we evaluate the database by performing a 
(deterministic) topological sort of the commit graph. Then, if we encounter
an operation which would violate an axiom on application, we just ignore it.
This means that the database is always in a valid state and everyone who has
seen the same set of commits will agree on what that state is.

Now, this is also potentially very unsatisfactory, it means that you can
arbitrarily lose a bunch of work because you happened to lose the conflict
resolution. But that's good, the point of this experiment is to find out what
the problems are.

### Automerge

Under the hood we need some way of shipping this commit graph around. The 
strategy in this project is to encode each operation as a JSON string and store
it in an Automerge document keyed by its hash. This is effectively using 
Automerge as a kind of dumb storage layer.

we also need access to the theory in order to validate operations, so we store
the theory in the same document as the operations. Thus, loading a database
means first loading the theory from the document, then all the operations.

Longer term we will probably want to go a layer below Automerge and use
the commit graph and columnar encoding machinery it uses directly, but for
now we have an end to end demo we can shout at.
