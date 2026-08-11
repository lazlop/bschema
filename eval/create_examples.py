"""Renders the `example_turtle` representation for every bschema output
already produced by `create_bschema.py`, without rerunning the algorithm.

Reads each `bschema/<threshold>/<building>.ttl` class graph together with
its matching `bschema-members/<threshold>/<building>.ttl` member graph, and
writes the example rendering to `bschema-examples/<threshold>/<building>.ttl`.
"""

import os

from rdflib import Graph
from bschema_rs import example_turtle

CLASS_GRAPH_DIR = "bschema"
MEMBER_GRAPH_DIR = "bschema-members"
OUTPUT_DIR = "bschema-examples"
EXAMPLE_COUNT = 2


def threshold_dirs(class_graph_dir):
    for name in sorted(os.listdir(class_graph_dir)):
        path = os.path.join(class_graph_dir, name)
        if os.path.isdir(path):
            yield name


def parse_turtle(path):
    g = Graph(store="Oxigraph")
    g.parse(path, format="ttl")
    return g


if __name__ == "__main__":
    for threshold in threshold_dirs(CLASS_GRAPH_DIR):
        class_dir = os.path.join(CLASS_GRAPH_DIR, threshold)
        member_dir = os.path.join(MEMBER_GRAPH_DIR, threshold)
        output_dir = os.path.join(OUTPUT_DIR, threshold)

        for file_name in sorted(os.listdir(class_dir)):
            if not file_name.endswith(".ttl"):
                continue

            member_path = os.path.join(member_dir, file_name)
            if not os.path.exists(member_path):
                print(f"Skipping {threshold}/{file_name}: no matching member graph at {member_path}")
                continue

            class_graph = parse_turtle(os.path.join(class_dir, file_name))
            member_graph = parse_turtle(member_path)

            turtle = example_turtle(class_graph, member_graph, EXAMPLE_COUNT)

            os.makedirs(output_dir, exist_ok=True)
            output_path = os.path.join(output_dir, file_name)
            with open(output_path, "w") as f:
                f.write(turtle)

            print(f"Wrote {output_path}")
