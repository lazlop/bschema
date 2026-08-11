use bschema_core::algorithm::create_bschema;
use bschema_core::graph::RdfGraph;
use oxigraph::io::RdfFormat;

const TTL: &str = r#"
    @prefix ex: <urn:example#> .
    @prefix brick: <https://brickschema.org/schema/Brick#> .

    ex:AHU_1 a brick:AHU ; brick:hasPoint ex:point_1 .
    ex:AHU_2 a brick:AHU ; brick:hasPoint ex:point_2 .
    ex:point_1 a brick:Sensor .
    ex:point_2 a brick:Sensor .
"#;

/// Finds the single `... brick:hasPoint ... .` line in the example Turtle
/// text (there's exactly one `hasPoint` class-pattern triple for this data).
fn has_point_line(turtle: &str) -> &str {
    turtle
        .lines()
        .find(|l| l.contains("hasPoint"))
        .unwrap_or_else(|| panic!("expected a hasPoint line in:\n{turtle}"))
}

#[test]
fn substitutes_bs_classes_with_member_collections() {
    let data_graph = RdfGraph::parse_str(TTL, RdfFormat::Turtle).unwrap();
    let result = create_bschema(&data_graph, 10, None, true, true).unwrap();
    let turtle = result.example_turtle(2).unwrap();

    let line = has_point_line(&turtle);

    // Both AHUs and both points should show up as real ex: instance names,
    // grouped into parenthesized Turtle collections on each side.
    assert!(line.contains("(ex:AHU_1 ex:AHU_2)") || line.contains("(ex:AHU_2 ex:AHU_1)"));
    assert!(line.contains("(ex:point_1 ex:point_2)") || line.contains("(ex:point_2 ex:point_1)"));
    assert!(line.contains("brick:hasPoint"));
    assert!(line.trim_end().ends_with('.'));
}

#[test]
fn example_count_caps_the_collection_size() {
    let data_graph = RdfGraph::parse_str(TTL, RdfFormat::Turtle).unwrap();
    let result = create_bschema(&data_graph, 10, None, true, true).unwrap();
    let turtle = result.example_turtle(1).unwrap();

    let line = has_point_line(&turtle);
    // Exactly one member per side: a singleton collection, not a pair.
    assert!(line.contains("(ex:AHU_1)") || line.contains("(ex:AHU_2)"));
    assert!(line.contains("(ex:point_1)") || line.contains("(ex:point_2)"));
    assert!(!line.contains("AHU_1 ex:AHU_2") && !line.contains("AHU_2 ex:AHU_1"));
}

#[test]
fn falls_back_to_blank_syntax_for_a_raw_skolem_iri_in_an_already_stale_member_graph() {
    // Simulates a member_graph produced by an older version of
    // create_bschema (before blank-origin skolem nodes were reversed back
    // to real blank nodes) and only reloaded here - example_turtle should
    // still render it as a blank node, not leak the internal skolem IRI.
    let class_graph = RdfGraph::parse_str(
        "@prefix ex: <urn:example#> . @prefix bs: <urn:bschema#> . bs:Sensor_1 ex:hasSpec bs:Spec_1 .",
        RdfFormat::Turtle,
    )
    .unwrap();
    let member_graph = RdfGraph::parse_str(
        "@prefix bs: <urn:bschema#> . @prefix skolem: <urn:bschema-rs:skolem:> . \
         bs:Spec_1 <http://www.w3.org/2000/01/rdf-schema#member> skolem:abc123 .",
        RdfFormat::Turtle,
    )
    .unwrap();

    let turtle = bschema_core::examples::example_turtle(&class_graph, &member_graph, 2).unwrap();
    assert!(!turtle.contains("skolem"), "should not leak the skolem IRI, got:\n{turtle}");
    assert!(turtle.contains("(_:abc123)"), "should render it as a blank node, got:\n{turtle}");
}

const TTL_WITH_BLANK_NODES: &str = r#"
    @prefix ex: <urn:example#> .

    ex:sensor1 ex:hasSpec _:b1 .
    ex:sensor2 ex:hasSpec _:b2 .
    _:b1 a ex:Spec .
    _:b2 a ex:Spec .
"#;

#[test]
fn renders_blank_node_members_as_turtle_blank_nodes_not_skolem_iris() {
    let data_graph = RdfGraph::parse_str(TTL_WITH_BLANK_NODES, RdfFormat::Turtle).unwrap();
    let result = create_bschema(&data_graph, 10, None, true, true).unwrap();
    let turtle = result.example_turtle(2).unwrap();

    assert!(
        !turtle.contains("skolem"),
        "example turtle should not leak skolem placeholder IRIs, got:\n{turtle}"
    );

    let line = turtle
        .lines()
        .find(|l| l.contains("hasSpec"))
        .unwrap_or_else(|| panic!("expected a hasSpec line in:\n{turtle}"));
    // Both blank-node specs should show up as a collection of real Turtle
    // blank nodes, e.g. "(_:b0 _:b1)", not resource IRIs.
    assert!(line.contains("(_:"), "expected a blank-node collection in: {line}");
}

#[test]
fn is_deterministic_across_runs() {
    let data_graph = RdfGraph::parse_str(TTL, RdfFormat::Turtle).unwrap();
    let result = create_bschema(&data_graph, 10, None, true, true).unwrap();

    let first = result.example_turtle(2).unwrap();
    let second = result.example_turtle(2).unwrap();
    assert_eq!(first, second, "same input should deterministically pick the same members in the same order");
}
