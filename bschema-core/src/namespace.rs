//! Namespace constants, ported from `graph-pattern-id/bschema/namespaces.py`.

use oxigraph::model::NamedNode;
use std::sync::LazyLock;

macro_rules! ns_base {
    ($name:ident, $iri:expr) => {
        pub const $name: &str = $iri;
    };
}

ns_base!(BS_BASE, "urn:bschema#");
ns_base!(EX_BASE, "urn:example#");
ns_base!(HPF_BASE, "urn:hpflex#");
ns_base!(HPFS_BASE, "urn:hpflex/shapes#");
ns_base!(PARAM_BASE, "urn:___param___#");
ns_base!(BRICK_BASE, "https://brickschema.org/schema/Brick#");
ns_base!(TAG_BASE, "https://brickschema.org/schema/BrickTag#");
ns_base!(BSH_BASE, "https://brickschema.org/schema/BrickShape#");
ns_base!(REF_BASE, "https://brickschema.org/schema/Brick/ref#");
ns_base!(REC_BASE, "https://w3id.org/rec#");
ns_base!(S4BLDG_BASE, "https://saref.etsi.org/saref4bldg#");
ns_base!(S4ENER_BASE, "https://saref.etsi.org/saref4ener#");
ns_base!(SAREF_BASE, "https://saref.etsi.org/core#");
ns_base!(OWL_BASE, "http://www.w3.org/2002/07/owl#");
ns_base!(RDF_BASE, "http://www.w3.org/1999/02/22-rdf-syntax-ns#");
ns_base!(RDFS_BASE, "http://www.w3.org/2000/01/rdf-schema#");
ns_base!(SKOS_BASE, "http://www.w3.org/2004/02/skos/core#");
ns_base!(SH_BASE, "http://www.w3.org/ns/shacl#");
ns_base!(XSD_BASE, "http://www.w3.org/2001/XMLSchema#");
ns_base!(BOB_BASE, "http://data.ashrae.org/standard223/si-builder#");
ns_base!(QUDT_BASE, "http://qudt.org/schema/qudt/");
ns_base!(QK_BASE, "http://qudt.org/vocab/quantitykind/");
ns_base!(DV_BASE, "http://qudt.org/vocab/dimensionvector/");
ns_base!(UNIT_BASE, "http://qudt.org/vocab/unit/");
ns_base!(BACNET_BASE, "http://data.ashrae.org/bacnet/2020#");
ns_base!(S223_BASE, "http://data.ashrae.org/standard223#");
ns_base!(BM_BASE, "https://nrel.gov/BuildingMOTIF#");
ns_base!(CONSTRAINT_BASE, "https://nrel.gov/BuildingMOTIF/constraints#");

/// Base IRI used to skolemize blank nodes. Distinct from rdflib's own
/// skolemization base so the two implementations never collide, but the
/// same "contains this substring => treat as an anonymous/bnode-origin name"
/// check in [`crate::util::common_pattern`]-derived naming applies here too.
pub const BNODE_BASE: &str = "urn:bschema-rs:skolem:";

/// Base IRI used to skolemize literals, so they can be grouped by 1-hop
/// topology (incoming edges + datatype) the same way named/blank node
/// subjects are. Distinct from [`BNODE_BASE`] so the two kinds of synthetic
/// identity never collide and can be told apart when naming groups.
pub const LITERAL_SKOLEM_BASE: &str = "urn:bschema-rs:skolem-literal:";

/// Build a `NamedNode` in a given namespace without IRI validation, mirroring
/// rdflib's lenient `Namespace.__getitem__` / `URIRef` construction.
pub fn ns(base: &str, local: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("{base}{local}"))
}

macro_rules! named_node_const {
    ($name:ident, $iri:expr) => {
        pub static $name: LazyLock<NamedNode> = LazyLock::new(|| NamedNode::new($iri).unwrap());
    };
}

// rdf:type, used pervasively enough to deserve its own alias (`A` in the Python code).
named_node_const!(A, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type");

named_node_const!(OWL_ONTOLOGY, "http://www.w3.org/2002/07/owl#Ontology");
named_node_const!(RDF_SEQ, "http://www.w3.org/1999/02/22-rdf-syntax-ns#Seq");
named_node_const!(
    RDFS_MEMBER,
    "http://www.w3.org/2000/01/rdf-schema#member"
);
named_node_const!(
    RDFS_LITERAL,
    "http://www.w3.org/2000/01/rdf-schema#Literal"
);
named_node_const!(
    RDFS_RESOURCE,
    "http://www.w3.org/2000/01/rdf-schema#Resource"
);

// Predicates whose object is itself treated as "the class" when building the
// class-pattern graph (see `create_class_pattern`), ported from
// `create_class_pattern`'s `named_node_predicates` list in bschema.py.
named_node_const!(S223_HAS_ASPECT, "http://data.ashrae.org/standard223#hasAspect");
named_node_const!(
    S223_HAS_ENUMERATION_KIND,
    "http://data.ashrae.org/standard223#hasEnumerationKind"
);
named_node_const!(
    S223_HAS_QUANTITY_KIND,
    "http://data.ashrae.org/standard223#hasQuantityKind"
);
named_node_const!(S223_HAS_UNIT, "http://data.ashrae.org/standard223#hasUnit");
named_node_const!(S223_HAS_MEDIUM, "http://data.ashrae.org/standard223#hasMedium");
named_node_const!(
    S223_OF_CONSTITUENT,
    "http://data.ashrae.org/standard223#ofConstituent"
);
named_node_const!(QUDT_HAS_UNIT, "http://qudt.org/schema/qudt/hasUnit");
named_node_const!(S223_HAS_ROLE, "http://data.ashrae.org/standard223#hasRole");
named_node_const!(S223_HAS_DOMAIN, "http://data.ashrae.org/standard223#hasDomain");
named_node_const!(BRICK_HAS_UNIT, "https://brickschema.org/schema/Brick#hasUnit");
named_node_const!(
    QUDT_HAS_QUANTITY_KIND,
    "http://qudt.org/schema/qudt/hasQuantityKind"
);

pub fn named_node_predicates() -> [&'static NamedNode; 11] {
    [
        &S223_HAS_ASPECT,
        &S223_HAS_ENUMERATION_KIND,
        &S223_HAS_QUANTITY_KIND,
        &S223_HAS_UNIT,
        &S223_HAS_MEDIUM,
        &S223_OF_CONSTITUENT,
        &QUDT_HAS_UNIT,
        &S223_HAS_ROLE,
        &S223_HAS_DOMAIN,
        &BRICK_HAS_UNIT,
        &QUDT_HAS_QUANTITY_KIND,
    ]
}

/// The `urn:example#` ontology declaration subject that `create_bschema`
/// strips before processing, matching bschema.py's
/// `original_data_graph.remove((URIRef('urn:example#'), A, OWL.Ontology))`.
pub fn ex_ontology_subject() -> NamedNode {
    NamedNode::new_unchecked(EX_BASE)
}

pub fn bind_prefixes(store: &oxigraph::store::Store) {
    // Oxigraph's Store has no prefix-map concept (unlike rdflib's Graph);
    // prefixes only matter at serialization time and Turtle output is
    // produced with abbreviations computed from this table by the caller.
    let _ = store;
}

/// `(prefix, namespace_iri)` pairs mirroring `namespaces.py`'s `namespace_dict`,
/// used to shorten IRIs when serializing Turtle output.
pub fn prefix_table() -> Vec<(&'static str, &'static str)> {
    vec![
        ("xsd", XSD_BASE),
        ("rdf", RDF_BASE),
        ("owl", OWL_BASE),
        ("rdfs", RDFS_BASE),
        ("skos", SKOS_BASE),
        ("sh", SH_BASE),
        ("quantitykind", QK_BASE),
        ("qudt", QUDT_BASE),
        ("unit", UNIT_BASE),
        ("ref", REF_BASE),
        ("rec", REC_BASE),
        ("brick", BRICK_BASE),
        ("tag", TAG_BASE),
        ("bsh", BSH_BASE),
        ("P", PARAM_BASE),
        ("constraint", CONSTRAINT_BASE),
        ("bmotif", BM_BASE),
        ("hpflex", HPF_BASE),
        ("hpfs", HPFS_BASE),
        ("s223", S223_BASE),
        ("ex", EX_BASE),
        ("bs", BS_BASE),
        ("bob", BOB_BASE),
        ("bacnet", BACNET_BASE),
        ("s4bldg", S4BLDG_BASE),
        ("s4ener", S4ENER_BASE),
        ("saref", SAREF_BASE),
    ]
}
