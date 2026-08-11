use bschema_core::algorithm::create_bschema;
use bschema_core::graph::RdfGraph;
use oxigraph::io::RdfFormat;
use oxigraph::model::{NamedOrBlankNode, Term};
use std::collections::HashSet;

const TTL: &str = r#"
    @prefix ex: <urn:example#> .
    @prefix s223: <http://data.ashrae.org/standard223#> .

    ex:sensor1 a s223:Sensor ; s223:hasUnit s223:UnitA .
    ex:sensor2 a s223:Sensor ; s223:hasUnit s223:UnitA .
    ex:sensor3 a s223:Sensor ; s223:hasUnit s223:UnitA .
    ex:actuator1 a s223:Actuator ; s223:hasRole s223:RoleA .
"#;

#[test]
fn groups_isomorphic_one_hop_patterns_and_compresses() {
    let data_graph = RdfGraph::parse_str(TTL, RdfFormat::Turtle).unwrap();
    let original_len = data_graph.len();
    assert!(original_len > 0);

    let result = create_bschema(&data_graph, 10, None, true, true).unwrap();

    assert!(result.class_graph.len() > 0);
    assert!(result.class_graph.len() < original_len, "bschema should compress the graph");

    // sensor1/2/3 share an identical one-hop pattern and should collapse
    // into a single member-graph group of size 3.
    let member_triples = result.member_graph.triples();
    let seq_count = member_triples
        .iter()
        .filter(|t| t.predicate.as_str().ends_with("22-rdf-syntax-ns#type"))
        .count();
    // one Seq group per distinct equivalence class found
    assert!(seq_count >= 1);

    let sensor_group_size = member_triples
        .iter()
        .filter(|t| t.predicate.as_str().ends_with("rdf-schema#member"))
        .filter(|t| {
            let obj = t.object.to_string();
            obj.contains("sensor1") || obj.contains("sensor2") || obj.contains("sensor3")
        })
        .count();
    assert_eq!(sensor_group_size, 3, "the three isomorphic sensors should be grouped together");
}

#[test]
fn similarity_threshold_zero_is_single_pass() {
    let data_graph = RdfGraph::parse_str(TTL, RdfFormat::Turtle).unwrap();
    let result = create_bschema(&data_graph, 10, Some(0.0), true, true).unwrap();
    assert_eq!(result.iterations, 0);
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
fn groups_literals_by_topology_and_reports_original_values() {
    let data_graph = RdfGraph::parse_str(TTL_WITH_LITERALS, RdfFormat::Turtle).unwrap();
    let result = create_bschema(&data_graph, 10, None, true, true).unwrap();

    let member_triples = result.member_graph.triples();

    // No skolem placeholder should leak into the reported output; members
    // should be the real literal values.
    assert!(
        member_triples.iter().all(|t| !t.object.to_string().contains("skolem-literal")),
        "member graph should report original literal values, not skolem stand-ins"
    );

    let literal_values: HashSet<String> = member_triples
        .iter()
        .filter(|t| t.predicate.as_str().ends_with("rdf-schema#member"))
        .filter_map(|t| match &t.object {
            Term::Literal(l) => Some(l.value().to_string()),
            _ => None,
        })
        .collect();

    for v in ["22.5", "23.1", "19.8"] {
        assert!(
            literal_values.contains(v),
            "expected literal value {v} to be reported in the member graph, got {literal_values:?}"
        );
    }

    // The three distinct temperature readings share 1-hop topology (same
    // datatype, same incoming sensor/hasValue edge) and should collapse
    // into a single literal-class group, not stay unmerged.
    let literal_group_size = member_triples
        .iter()
        .filter(|t| t.predicate.as_str().ends_with("rdf-schema#member"))
        .filter(|t| matches!(&t.object, Term::Literal(_)))
        .count();
    assert_eq!(literal_group_size, 3, "the three isomorphic literal values should be grouped together");

    // The class graph should reference a derived literal class for
    // s223:hasValue, not collapse to the generic rdfs:Literal catch-all.
    let class_triples = result.class_graph.triples();
    let has_value_pattern = class_triples
        .iter()
        .find(|t| t.predicate.as_str().ends_with("hasValue"))
        .expect("hasValue should appear in the class graph");
    match &has_value_pattern.object {
        oxigraph::model::Term::NamedNode(n) => {
            assert!(
                !n.as_str().ends_with("rdf-schema#Literal"),
                "hasValue's object class should not collapse to generic rdfs:Literal, got {n}"
            );
        }
        other => panic!("expected hasValue's class-pattern object to be a NamedNode, got {other:?}"),
    }
}

const TTL_WITH_BLANK_NODES: &str = r#"
    @prefix ex: <urn:example#> .

    ex:sensor1 ex:hasSpec _:b1 .
    ex:sensor2 ex:hasSpec _:b2 .
    _:b1 a ex:Spec .
    _:b2 a ex:Spec .
"#;

#[test]
fn groups_blank_nodes_by_topology_and_reports_as_blank_nodes() {
    let data_graph = RdfGraph::parse_str(TTL_WITH_BLANK_NODES, RdfFormat::Turtle).unwrap();
    let result = create_bschema(&data_graph, 10, None, true, true).unwrap();

    let member_triples = result.member_graph.triples();

    // No skolem placeholder IRI should leak into the reported output;
    // blank-node members should be reported as real Turtle blank nodes.
    assert!(
        member_triples.iter().all(|t| !t.object.to_string().contains("bschema-rs:skolem:")),
        "member graph should report original blank nodes, not skolem stand-ins, got {member_triples:?}"
    );

    let blank_member_count = member_triples
        .iter()
        .filter(|t| t.predicate.as_str().ends_with("rdf-schema#member"))
        .filter(|t| matches!(&t.object, Term::BlankNode(_)))
        .count();
    assert_eq!(blank_member_count, 2, "the two isomorphic blank-node specs should be grouped together");
}

#[test]
fn threshold_zero_member_graph_is_keyed_consistently_with_class_graph() {
    // Regression test: `similarity_threshold == Some(0.0)` breaks out of
    // the iteration loop right after applying iteration 0's relabeling, so
    // the member graph must be keyed by that *applied* label - not by
    // `subject_classes`, which (only on this path) still describes each
    // group's class as of the *start* of iteration 0, before relabeling.
    // Getting this wrong means class_graph references a `bs:` class the
    // member graph has no entry for at all.
    let data_graph = RdfGraph::parse_str(TTL, RdfFormat::Turtle).unwrap();
    let result = create_bschema(&data_graph, 10, Some(0.0), true, true).unwrap();

    let member_classes: HashSet<String> = result
        .member_graph
        .triples()
        .into_iter()
        .filter(|t| t.predicate.as_str().ends_with("rdf-schema#member"))
        .filter_map(|t| match t.subject {
            NamedOrBlankNode::NamedNode(n) => Some(n.as_str().to_string()),
            NamedOrBlankNode::BlankNode(_) => None,
        })
        .collect();

    let mut checked_any = false;
    for t in result.class_graph.triples() {
        for term in [Term::from(t.subject.clone()), t.object.clone()] {
            if let Term::NamedNode(n) = &term {
                if n.as_str().starts_with("urn:bschema#") {
                    checked_any = true;
                    assert!(
                        member_classes.contains(n.as_str()),
                        "class_graph references {n} but member_graph has no members for it"
                    );
                }
            }
        }
    }
    assert!(checked_any, "test setup: expected at least one bs: class in class_graph");
}
