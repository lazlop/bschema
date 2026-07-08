# bschema-rs

A Rust reimplementation of [`bschema`](../../graph-pattern-id/bschema), using
[Oxigraph](https://github.com/oxigraph/oxigraph) for RDF storage/parsing, with
Python bindings (via [PyO3](https://pyo3.rs)/[maturin](https://www.maturin.rs)).

## Usage

```python
from rdflib import Graph
from bschema_rs import create_bschema

data_graph = Graph(store="Oxigraph")
data_graph.parse("model.ttl")

class_graph, member_graph, iterations = create_bschema(data_graph, iterations=10)
class_graph.serialize("model_bschema.ttl")
```

Or, to skip round-tripping through an rdflib graph, parse straight from a file
path in Rust:

```python
from bschema_rs import create_bschema_from_file

class_graph, member_graph, iterations = create_bschema_from_file("model.ttl")
```

## CLI

```sh
create-bschema-rs -i model.ttl
```

## Development

```sh
maturin develop --release
```
