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
    // A single-member, all-blank group collapses to a bare `[]`, not a
    // parenthesized list of one hash-labelled blank node.
    assert!(turtle.contains("ex:hasSpec []"), "should render it as a bare blank node, got:\n{turtle}");
}

const TTL_WITH_BLANK_NODES: &str = r#"
    @prefix ex: <urn:example#> .

    ex:sensor1 ex:hasSpec _:b1 .
    ex:sensor2 ex:hasSpec _:b2 .
    _:b1 a ex:Spec .
    _:b2 a ex:Spec .
"#;

#[test]
fn nests_an_all_blank_class_as_an_indented_property_list() {
    let data_graph = RdfGraph::parse_str(TTL_WITH_BLANK_NODES, RdfFormat::Turtle).unwrap();
    let result = create_bschema(&data_graph, 10, None, true, true).unwrap();
    let turtle = result.example_turtle(2).unwrap();

    assert!(
        !turtle.contains("skolem"),
        "example turtle should not leak skolem placeholder IRIs, got:\n{turtle}"
    );
    assert!(!turtle.contains("_:"), "should not leak a raw blank node label, got:\n{turtle}");

    // Two blank-node specs carry no more information than one, so instead
    // of listing members, the class shows what it itself asserts (`a
    // ex:Spec`) as a nested, indented Turtle blank-node property list.
    let expected = "(ex:sensor1 ex:sensor2) ex:hasSpec [\n    a ex:Spec\n] .";
    assert!(turtle.contains(expected), "expected:\n{expected}\n\ngot:\n{turtle}");
}

const TTL_WITH_REPEATED_PREDICATE: &str = r#"
    @prefix ex: <urn:example#> .
    @prefix brick: <https://brickschema.org/schema/Brick#> .

    ex:AHU_1 a brick:AHU ; brick:hasPoint ex:tempA_1, ex:tempB_1 .
    ex:AHU_2 a brick:AHU ; brick:hasPoint ex:tempA_2, ex:tempB_2 .
    ex:tempA_1 a brick:Temperature_Sensor .
    ex:tempA_2 a brick:Temperature_Sensor .
    ex:tempB_1 a brick:Setpoint .
    ex:tempB_2 a brick:Setpoint .
"#;

#[test]
fn groups_repeated_subject_predicate_pairs_with_an_object_list() {
    // The two AHUs each relate to two different point classes through the
    // same brick:hasPoint predicate - that's two class_graph rows sharing
    // (subject, predicate), which should collapse into one Turtle
    // statement using the standard object-list comma syntax, rather than
    // repeating the same subject list on two near-duplicate lines.
    let data_graph = RdfGraph::parse_str(TTL_WITH_REPEATED_PREDICATE, RdfFormat::Turtle).unwrap();
    let result = create_bschema(&data_graph, 10, None, true, true).unwrap();
    let turtle = result.example_turtle(2).unwrap();

    let has_point_lines: Vec<&str> = turtle.lines().filter(|l| l.contains("brick:hasPoint")).collect();
    assert_eq!(has_point_lines.len(), 1, "expected a single hasPoint statement, got:\n{turtle}");
    assert!(has_point_lines[0].trim_end().ends_with("brick:hasPoint"), "got:\n{turtle}");

    let comma_count = turtle.matches(" ,\n").count();
    assert_eq!(comma_count, 1, "expected exactly one comma-joined continuation, got:\n{turtle}");

    assert!(turtle.contains("(ex:tempA_1 ex:tempA_2)"), "got:\n{turtle}");
    assert!(turtle.contains("(ex:tempB_1 ex:tempB_2)"), "got:\n{turtle}");
}

const TTL_WITH_A_BLANK_LITERAL_MEMBER: &str = r#"
    @prefix ex: <urn:example#> .
    @prefix s223: <http://data.ashrae.org/standard223#> .

    ex:sensor1 a s223:Sensor ; s223:hasValue "" .
    ex:sensor2 a s223:Sensor ; s223:hasValue "22.5" .
    ex:sensor3 a s223:Sensor ; s223:hasValue "23.1" .
"#;

#[test]
fn prefers_non_blank_literals_as_examples() {
    // "" is a real member but an uninformative example; with a 2-example
    // budget and two non-blank alternatives available, it shouldn't
    // consume a slot that could show "22.5" or "23.1" instead.
    let data_graph = RdfGraph::parse_str(TTL_WITH_A_BLANK_LITERAL_MEMBER, RdfFormat::Turtle).unwrap();
    let result = create_bschema(&data_graph, 10, None, true, true).unwrap();
    let turtle = result.example_turtle(2).unwrap();

    assert!(turtle.contains("\"22.5\""), "got:\n{turtle}");
    assert!(turtle.contains("\"23.1\""), "got:\n{turtle}");
    assert!(!turtle.contains("(\"\""), "should not spend an example slot on a blank literal, got:\n{turtle}");
}

#[test]
fn still_shows_a_blank_literal_when_no_better_example_exists() {
    let data_graph = RdfGraph::parse_str(
        r#"
            @prefix ex: <urn:example#> .
            @prefix s223: <http://data.ashrae.org/standard223#> .
            ex:sensor1 a s223:Sensor ; s223:hasValue "" .
        "#,
        RdfFormat::Turtle,
    )
    .unwrap();
    let result = create_bschema(&data_graph, 10, None, true, true).unwrap();
    let turtle = result.example_turtle(2).unwrap();

    assert!(turtle.contains("hasValue (\"\")"), "the only member is blank, so it should still show up: got:\n{turtle}");
}

const TTL_WITH_UNKNOWN_NAMESPACE: &str = r#"
    @prefix bldg: <urn:bldg#> .

    bldg:AHU_1 bldg:hasPoint bldg:Point_1 .
    bldg:AHU_2 bldg:hasPoint bldg:Point_2 .
"#;

#[test]
fn auto_numbers_a_prefix_for_an_unknown_namespace() {
    // `urn:bldg#` isn't one of the crate's known namespaces (brick, s223,
    // ex, bs, ...), so it should get an auto-numbered ns1: binding instead
    // of showing up as a long bracketed <urn:bldg#...> IRI everywhere.
    let data_graph = RdfGraph::parse_str(TTL_WITH_UNKNOWN_NAMESPACE, RdfFormat::Turtle).unwrap();
    let result = create_bschema(&data_graph, 10, None, true, true).unwrap();
    let turtle = result.example_turtle(2).unwrap();

    assert!(turtle.contains("@prefix ns1: <urn:bldg#> ."), "expected an auto-numbered prefix header, got:\n{turtle}");
    // The only occurrence of the raw IRI should be in the @prefix header
    // itself; every use in the body should be abbreviated via ns1:.
    assert_eq!(
        turtle.matches("<urn:bldg#").count(),
        1,
        "should not fall back to full IRIs in the body once a prefix is bound, got:\n{turtle}"
    );
    assert!(turtle.contains("ns1:hasPoint"), "expected the predicate to use the auto-numbered prefix, got:\n{turtle}");
}

#[test]
fn strips_the_synthetic_rdfs_literal_bookkeeping_triples() {
    let data_graph = RdfGraph::parse_str(TTL_WITH_LITERALS, RdfFormat::Turtle).unwrap();
    let result = create_bschema(&data_graph, 10, None, true, true).unwrap();

    // The underlying class_graph does carry these (see
    // create_bschema.rs's literal test); example_turtle should filter them
    // out as an implementation artifact of RdfGraph::skolemize, not model
    // content worth showing a reader.
    assert!(
        result.class_graph.triples().iter().any(|t| t.object.to_string().contains("rdf-schema#Literal")),
        "test setup: expected class_graph to contain a `a rdfs:Literal` bookkeeping triple"
    );

    let turtle = result.example_turtle(2).unwrap();
    assert!(!turtle.contains("Literal"), "should not surface rdfs:Literal bookkeeping, got:\n{turtle}");
}

const TTL_WITH_LITERALS: &str = r#"
    @prefix ex: <urn:example#> .
    @prefix s223: <http://data.ashrae.org/standard223#> .
    @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

    ex:sensor1 a s223:Sensor ; s223:hasUnit s223:UnitA ; s223:hasValue "22.5"^^xsd:double .
    ex:sensor2 a s223:Sensor ; s223:hasUnit s223:UnitA ; s223:hasValue "23.1"^^xsd:double .
    ex:sensor3 a s223:Sensor ; s223:hasUnit s223:UnitA ; s223:hasValue "19.8"^^xsd:double .
"#;

#[test]
fn is_deterministic_across_runs() {
    let data_graph = RdfGraph::parse_str(TTL, RdfFormat::Turtle).unwrap();
    let result = create_bschema(&data_graph, 10, None, true, true).unwrap();

    let first = result.example_turtle(2).unwrap();
    let second = result.example_turtle(2).unwrap();
    assert_eq!(first, second, "same input should deterministically pick the same members in the same order");
}
