# bschema-rs

A Rust reimplementation of `bschema`, an algorithm that summarizes an RDF
building-metadata graph into a compact **class graph** describing the
repeated structural patterns in the data, plus a **member graph** mapping
each derived class back to the original entities it groups. It's built on
[Oxigraph](https://github.com/oxigraph/oxigraph) for RDF parsing/storage and
[Rayon](https://github.com/rayon-rs/rayon) for parallelism, with Python
bindings exposed via [PyO3](https://pyo3.rs)/[maturin](https://www.maturin.rs).

## How it works

Given a data graph, `create_bschema` iteratively:

1. Groups subjects by the isomorphism (or, with a `similarity_threshold`,
   high structural overlap) of their 1-hop class-pattern subgraph.
2. Assigns each group a fresh `bs:`-namespaced class.
3. Re-labels the graph with the new classes and repeats until the groupings
   stabilize or `iterations` is reached.

The result is a `class_graph` (the deduplicated class-level patterns — `H`
in the paper) and a `member_graph` (which original subjects belong to each
derived class — `M` in the paper).

## Repo layout

- `bschema-core/` — the core algorithm and RDF graph utilities (pure Rust,
  no Python dependency).
- `bschema-py/` — PyO3 bindings and the `bschema_rs` Python package
  (including a `create-bschema-rs` CLI).
- `eval/` — evaluation scripts/data used to benchmark bschema-rs against
  real building models (runtime and compression vs. graph size, across
  similarity thresholds).

## Installation

### Rust

```sh
cargo build --release
```

This builds the `bschema-core` and `bschema-py` crates as a Cargo workspace;
`bschema-core` can be used as a standalone Rust library.

### Python

The Python package is built with [maturin](https://www.maturin.rs). From
`bschema-py/`:

```sh
cd bschema-py
python -m venv .venv && source .venv/bin/activate
pip install maturin
maturin develop --release
```

This builds the Rust extension and installs the `bschema_rs` package into
your active virtualenv. Optionally install `rdflib` (used by the
higher-level Python API) via `pip install "bschema-rs[rdflib]"` once
published, or `pip install rdflib` directly during development.

## Usage in Python

```python
from rdflib import Graph
from bschema_rs import create_bschema

data_graph = Graph(store="Oxigraph")
data_graph.parse("model.ttl")

class_graph, member_graph, iterations = create_bschema(data_graph, iterations=10)
class_graph.serialize("model_bschema.ttl")
```

To skip round-tripping through an rdflib graph, parse directly from a file
path in Rust:

```python
from bschema_rs import create_bschema_from_file

class_graph, member_graph, iterations = create_bschema_from_file("model.ttl")
```

`create_bschema` / `create_bschema_from_file` accept:

- `iterations` (default `10`) — max number of relabeling passes.
- `similarity_threshold` (default `None`) — if set, groups subjects whose
  class-pattern subgraphs overlap above this ratio (0–1), instead of
  requiring exact isomorphism.
- `remove_added_labels` (default `True`) — strip the `bs:` classes the
  algorithm added from the output class graph.
- `use_original_names` (default `True`) — derive new class names from the
  common substring of grouped subjects' original IRIs, instead of
  versioning the existing class name.

### CLI

```sh
create-bschema-rs -i model.ttl -o model_bschema.ttl -t 0.5 -r 10
```

- `-i/--input_file` — path to the input RDF file (required).
- `-o/--output_file` — output path (defaults to `<input>_bschema.<ext>`).
- `-t/--threshold` — similarity threshold (try `0.5`).
- `-r/--iterations` — number of iterations (default `10`).
- `-d/--delete_added_classes` — delete the classes added by the algorithm.

## Evaluation

`eval/` contains scripts for benchmarking bschema-rs across a set of
anonymized building models (`eval/eval_buildings/`) and similarity
thresholds. `eval/create_bschema.py` runs the algorithm over each model and
records runtime/compression stats; `eval/plot_bschema.py` plots the results.

## Development

Run the Rust test suite with:

```sh
cargo test
```

and rebuild the Python extension after Rust changes with:

```sh
cd bschema-py && maturin develop --release
```
