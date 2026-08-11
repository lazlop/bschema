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
fn is_deterministic_across_runs() {
    let data_graph = RdfGraph::parse_str(TTL, RdfFormat::Turtle).unwrap();
    let result = create_bschema(&data_graph, 10, None, true, true).unwrap();

    let first = result.example_turtle(2).unwrap();
    let second = result.example_turtle(2).unwrap();
    assert_eq!(first, second, "same input should deterministically pick the same members in the same order");
}
