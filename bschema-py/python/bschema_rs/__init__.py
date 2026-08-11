"""Python bindings for bschema-rs, a Rust/Oxigraph reimplementation of bschema.

`create_bschema` mirrors the signature and return shape of
`graph-pattern-id/bschema/bschema.py`'s `create_bschema`, so existing callers
can switch to this package by changing only their import.
"""

from __future__ import annotations

from typing import Optional, Tuple

from ._bschema_rs import create_bschema as _create_bschema_str
from ._bschema_rs import create_bschema_from_file as _create_bschema_file
from ._bschema_rs import example_turtle as _example_turtle

__all__ = ["create_bschema", "create_bschema_from_file", "example_turtle", "bind_prefixes"]

_PREFIXES = {
    "xsd": "http://www.w3.org/2001/XMLSchema#",
    "rdf": "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
    "owl": "http://www.w3.org/2002/07/owl#",
    "rdfs": "http://www.w3.org/2000/01/rdf-schema#",
    "skos": "http://www.w3.org/2004/02/skos/core#",
    "sh": "http://www.w3.org/ns/shacl#",
    "quantitykind": "http://qudt.org/vocab/quantitykind/",
    "qudt": "http://qudt.org/schema/qudt/",
    "unit": "http://qudt.org/vocab/unit/",
    "ref": "https://brickschema.org/schema/Brick/ref#",
    "rec": "https://w3id.org/rec#",
    "brick": "https://brickschema.org/schema/Brick#",
    "tag": "https://brickschema.org/schema/BrickTag#",
    "bsh": "https://brickschema.org/schema/BrickShape#",
    "P": "urn:___param___#",
    "constraint": "https://nrel.gov/BuildingMOTIF/constraints#",
    "bmotif": "https://nrel.gov/BuildingMOTIF#",
    "hpflex": "urn:hpflex#",
    "hpfs": "urn:hpflex/shapes#",
    "s223": "http://data.ashrae.org/standard223#",
    "ex": "urn:example#",
    "bs": "urn:bschema#",
    "bob": "http://data.ashrae.org/standard223/si-builder#",
    "bacnet": "http://data.ashrae.org/bacnet/2020#",
    "s4bldg": "https://saref.etsi.org/saref4bldg#",
    "s4ener": "https://saref.etsi.org/saref4ener#",
    "saref": "https://saref.etsi.org/core#",
}


def bind_prefixes(graph) -> None:
    """Binds bschema's common namespace prefixes on an rdflib.Graph."""
    for prefix, namespace in _PREFIXES.items():
        graph.bind(prefix, namespace)


def _parse_turtle(data: str):
    from rdflib import Graph

    graph = Graph(store="Oxigraph")
    graph.parse(data=data, format="turtle")
    bind_prefixes(graph)
    return graph


def create_bschema(
    data_graph,
    iterations: int = 10,
    similarity_threshold: Optional[float] = None,
    remove_added_labels: bool = True,
    use_original_names: bool = True,
) -> Tuple[object, object, int]:
    """Summarizes an rdflib.Graph into `(class_graph, member_graph, iterations)`.

    `data_graph` is serialized to Turtle and handed to the Rust
    implementation; the two returned graphs are rdflib `Graph` objects
    parsed back from the Rust side's Turtle output.
    """
    ttl = data_graph.serialize(format="turtle")
    class_ttl, member_ttl, iterations_run = _create_bschema_str(
        ttl,
        "turtle",
        iterations,
        similarity_threshold,
        remove_added_labels,
        use_original_names,
    )
    return _parse_turtle(class_ttl), _parse_turtle(member_ttl), iterations_run


def _serialize_turtle_safe(graph) -> str:
    """Serializes `graph` as Turtle, falling back to N-Triples (still valid
    Turtle syntax, just unprettified) if rdflib's Turtle serializer crashes -
    e.g. on "NaN"^^xsd:double placeholder values, where rdflib's property-list
    sort compares Literal values and Python's Decimal("NaN") comparison
    raises InvalidOperation instead of just sorting arbitrarily."""
    try:
        return graph.serialize(format="turtle")
    except Exception:
        return graph.serialize(format="nt")


def example_turtle(class_graph, member_graph, example_count: int = 2) -> str:
    """Renders `(class_graph, member_graph)` (as returned by `create_bschema`)
    as "example" Turtle text: each `bs:` class node is replaced by a
    parenthesized Turtle collection of up to `example_count` of its real
    members, e.g. `bs:AHU_v1 brick:hasPoint bs:Point_v1` becomes
    `(ex:AHU_1 ex:AHU_2) brick:hasPoint (ex:point_1 ex:point_2) .`.

    Returned as a plain Turtle string, not an rdflib Graph: a collection
    used as a triple's subject can't be losslessly round-tripped through a
    generic Turtle writer (rdflib's collection-folding only applies when a
    list is used as an object), so re-parsing would only get you back an
    uglier equivalent, not this compact form.
    """
    class_ttl = _serialize_turtle_safe(class_graph)
    member_ttl = _serialize_turtle_safe(member_graph)
    return _example_turtle(class_ttl, member_ttl, example_count)


def create_bschema_from_file(
    path: str,
    iterations: int = 10,
    similarity_threshold: Optional[float] = None,
    remove_added_labels: bool = True,
    use_original_names: bool = True,
) -> Tuple[object, object, int]:
    """Same as `create_bschema`, but the input file is read and parsed in Rust."""
    class_ttl, member_ttl, iterations_run = _create_bschema_file(
        path,
        iterations,
        similarity_threshold,
        remove_added_labels,
        use_original_names,
    )
    return _parse_turtle(class_ttl), _parse_turtle(member_ttl), iterations_run
