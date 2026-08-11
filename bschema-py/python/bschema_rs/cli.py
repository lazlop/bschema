#!/usr/bin/env python
import argparse
import os

from . import create_bschema_from_file, example_turtle


def _with_suffix(path: str, suffix: str) -> str:
    base, extension = os.path.splitext(path)
    return f"{base}{suffix}{extension}"


def run():
    parser = argparse.ArgumentParser(
        description="Create the bschema for a given RDF file (bschema-rs implementation)"
    )
    parser.add_argument("-i", "--input_file", required=True, help="Path to the input RDF file")
    parser.add_argument("-o", "--output_file", help="Path to the output RDF file")
    parser.add_argument(
        "-t",
        "--threshold",
        type=float,
        help="Similarity threshold, lower thresholds let groupings with not the exact same "
        "representation be grouped together (try 0.5)",
    )
    parser.add_argument("-d", "--delete_added_classes", action="store_true", help="Delete added classes")
    parser.add_argument("-r", "--iterations", type=int, default=10, help="Number of iterations")
    parser.add_argument(
        "-e",
        "--examples",
        type=int,
        nargs="?",
        const=2,
        default=None,
        metavar="N",
        help="Also write a '<output>_examples.<ext>' Turtle file with each bs: class replaced "
        "by up to N (default 2) of its real members in Turtle list syntax, e.g. "
        "'(ex:AHU_1 ex:AHU_2) brick:hasPoint (ex:point_1 ex:point_2) .'",
    )

    args = parser.parse_args()

    class_graph, member_graph, _iterations = create_bschema_from_file(
        args.input_file, args.iterations, args.threshold, args.delete_added_classes
    )

    if args.output_file is None:
        base, extension = os.path.splitext(args.input_file)
        if args.threshold is not None:
            output_file = f"{base}_threshold_{args.threshold}_bschema{extension}"
        else:
            output_file = f"{base}_bschema{extension}"
    else:
        output_file = args.output_file

    class_graph.serialize(output_file)

    if args.examples is not None:
        example_file = _with_suffix(output_file, "_examples")
        with open(example_file, "w") as f:
            f.write(example_turtle(class_graph, member_graph, args.examples))
        print(f"Wrote {args.examples}-example Turtle summary to {example_file}")

    from rdflib import Graph

    original = Graph(store="Oxigraph")
    original.parse(args.input_file)
    print(f"Compressed to {len(class_graph) / len(original) * 100:.2f}% of its original size")


if __name__ == "__main__":
    run()
