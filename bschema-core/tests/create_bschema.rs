use bschema_core::algorithm::create_bschema;
use bschema_core::graph::RdfGraph;
use oxigraph::io::RdfFormat;

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
