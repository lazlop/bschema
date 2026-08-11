//! A thin, `rdflib.Graph`-shaped wrapper around an in-memory Oxigraph [`Store`].
//!
//! Every "graph" in the original Python (`data_graph`, per-node subgraphs,
//! class graphs, the member graph) was an `rdflib.Graph(store='Oxigraph')`;
//! this type plays the same role here, always operating on the store's
//! default graph (bschema never uses named graphs).

use crate::error::Result;
use oxigraph::io::{RdfFormat, RdfParser, RdfSerializer};
use oxigraph::model::{BlankNode, GraphNameRef, Literal, NamedNode, NamedOrBlankNode, Term, Triple};
use oxigraph::store::Store;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;

pub struct RdfGraph {
    pub(crate) store: Store,
}

impl RdfGraph {
    pub fn new() -> Result<Self> {
        Ok(Self { store: Store::new()? })
    }

    pub fn parse_file(path: &Path) -> Result<Self> {
        let format = format_from_extension(path)?;
        let graph = Self::new()?;
        let reader = std::fs::File::open(path)?;
        graph.store.load_from_reader(RdfParser::from_format(format), reader)?;
        Ok(graph)
    }

    pub fn parse_str(data: &str, format: RdfFormat) -> Result<Self> {
        let graph = Self::new()?;
        graph.store.load_from_slice(RdfParser::from_format(format), data)?;
        Ok(graph)
    }

    pub fn len(&self) -> usize {
        self.store.len().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn insert(&self, triple: &Triple) {
        self.store
            .insert(triple.as_ref().in_graph(GraphNameRef::DefaultGraph))
            .expect("in-memory store insert cannot fail");
    }

    pub fn remove(&self, triple: &Triple) {
        self.store
            .remove(triple.as_ref().in_graph(GraphNameRef::DefaultGraph))
            .expect("in-memory store remove cannot fail");
    }

    pub fn contains(&self, triple: &Triple) -> bool {
        self.store
            .contains(triple.as_ref().in_graph(GraphNameRef::DefaultGraph))
            .unwrap_or(false)
    }

    /// All triples in the default graph, in store iteration order.
    pub fn triples(&self) -> Vec<Triple> {
        self.store
            .quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph))
            .filter_map(|q| q.ok())
            .map(|q| Triple::new(q.subject, q.predicate, q.object))
            .collect()
    }

    /// Objects `o` such that `(subject, predicate, o)` holds, i.e. rdflib's
    /// `graph.objects(subject, predicate)`.
    pub fn objects(&self, subject: &NamedOrBlankNode, predicate: &NamedNode) -> Vec<Term> {
        self.store
            .quads_for_pattern(
                Some(subject.as_ref()),
                Some(predicate.as_ref()),
                None,
                Some(GraphNameRef::DefaultGraph),
            )
            .filter_map(|q| q.ok())
            .map(|q| q.object)
            .collect()
    }

    /// `(predicate, object)` pairs for a given subject, i.e. rdflib's
    /// `graph.predicate_objects(subject)`.
    pub fn predicate_objects(&self, subject: &NamedOrBlankNode) -> Vec<(NamedNode, Term)> {
        self.store
            .quads_for_pattern(Some(subject.as_ref()), None, None, Some(GraphNameRef::DefaultGraph))
            .filter_map(|q| q.ok())
            .map(|q| (q.predicate, q.object))
            .collect()
    }

    /// `(subject, predicate)` pairs for a given object, i.e. rdflib's
    /// `graph.subject_predicates(object)`.
    pub fn subject_predicates(&self, object: &Term) -> Vec<(NamedOrBlankNode, NamedNode)> {
        self.store
            .quads_for_pattern(None, None, Some(object.as_ref()), Some(GraphNameRef::DefaultGraph))
            .filter_map(|q| q.ok())
            .map(|q| (q.subject, q.predicate))
            .collect()
    }

    /// Returns a copy of this graph with every blank node replaced by a
    /// deterministic named node, mirroring rdflib's `Graph.skolemize()`, and
    /// every literal *object* likewise replaced by a deterministic named
    /// node carrying a synthetic `<node> rdf:type rdfs:Literal` triple.
    /// bschema relies on this to make "which node is this" stable identity
    /// (a `NamedOrBlankNode` set/hashmap key) across the iterative
    /// class-reassignment loop in `create_bschema`; skolemizing literals the
    /// same way lets them be grouped by 1-hop topology (incoming edges: the
    /// (subject-class, predicate) role they're reached through) exactly
    /// like any other subject, instead of collapsing into one generic
    /// `rdfs:Literal` bucket in the class *pattern*.
    ///
    /// Unlike blank/named nodes, literal skolem identity is deliberately
    /// per-*occurrence* (keyed by (subject, predicate, literal)), not by
    /// value: merging every occurrence of a repeated value (e.g. a common
    /// `"false"^^xsd:boolean` or `"0"^^xsd:double` default) onto one shared
    /// node would give that node a large, heterogeneous incoming-edge set
    /// spanning many unrelated (subject-class, predicate) contexts - a node
    /// that essentially never matches any other node exactly, so it stays a
    /// singleton and gets a fresh label every iteration, destabilizing every
    /// subject that happens to share that value on every pass. Per-
    /// occurrence identity means a literal node only ever has one incoming
    /// edge, so it can only merge with other nodes reached the same way
    /// (same subject-class + predicate) - which is the topology grouping we
    /// actually want, without that shared-value hub effect. Returns the
    /// skolemized graph alongside a reverse map from each skolem node (blank
    /// *or* literal) back to the original term, so callers can undo the
    /// substitution when reporting results (e.g. the member graph): a
    /// blank-origin skolem reverses to the original `Term::BlankNode` (so it
    /// prints as `_:...` and reads as "this was a blank node", not an opaque
    /// `urn:bschema-rs:skolem:...` resource), a literal-origin skolem
    /// reverses to the original `Term::Literal`.
    pub fn skolemize(&self) -> Result<(Self, HashMap<NamedNode, Term>)> {
        let out = Self::new()?;
        let mut blank_mapping: HashMap<BlankNode, NamedNode> = HashMap::new();
        let mut skolem_reverse: HashMap<NamedNode, Term> = HashMap::new();
        let mut literal_skolem_nodes: Vec<NamedNode> = Vec::new();

        let skolem_blank = |b: &BlankNode,
                             mapping: &mut HashMap<BlankNode, NamedNode>,
                             reverse: &mut HashMap<NamedNode, Term>| {
            mapping
                .entry(b.clone())
                .or_insert_with(|| {
                    let node = NamedNode::new_unchecked(format!(
                        "{}{}",
                        crate::namespace::BNODE_BASE,
                        b.as_str()
                    ));
                    reverse.insert(node.clone(), Term::BlankNode(b.clone()));
                    node
                })
                .clone()
        };

        let skolem_literal = |subject: &NamedOrBlankNode, predicate: &NamedNode, l: &Literal| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            subject.hash(&mut hasher);
            predicate.hash(&mut hasher);
            l.hash(&mut hasher);
            NamedNode::new_unchecked(format!(
                "{}{:016x}",
                crate::namespace::LITERAL_SKOLEM_BASE,
                hasher.finish()
            ))
        };

        for t in self.triples() {
            let subject = match &t.subject {
                NamedOrBlankNode::BlankNode(b) => {
                    NamedOrBlankNode::NamedNode(skolem_blank(b, &mut blank_mapping, &mut skolem_reverse))
                }
                other => other.clone(),
            };
            let object = match &t.object {
                Term::BlankNode(b) => Term::NamedNode(skolem_blank(b, &mut blank_mapping, &mut skolem_reverse)),
                Term::Literal(l) => {
                    let node = skolem_literal(&t.subject, &t.predicate, l);
                    skolem_reverse.insert(node.clone(), Term::Literal(l.clone()));
                    literal_skolem_nodes.push(node.clone());
                    Term::NamedNode(node)
                }
                other => other.clone(),
            };
            out.insert(&Triple::new(subject, t.predicate, object));
        }

        for node in &literal_skolem_nodes {
            out.insert(&Triple::new(
                node.clone(),
                crate::namespace::A.clone(),
                crate::namespace::RDFS_LITERAL.clone(),
            ));
        }

        Ok((out, skolem_reverse))
    }

    pub fn serialize_turtle(&self) -> Result<String> {
        let bytes = self
            .store
            .dump_graph_to_writer(GraphNameRef::DefaultGraph, RdfSerializer::from_format(RdfFormat::Turtle), Vec::new())?;
        Ok(prefix_header() + &String::from_utf8_lossy(&bytes))
    }
}

pub(crate) fn prefix_header() -> String {
    crate::namespace::prefix_table()
        .into_iter()
        .map(|(prefix, iri)| format!("@prefix {prefix}: <{iri}> .\n"))
        .collect::<String>()
        + "\n"
}

fn format_from_extension(path: &Path) -> Result<RdfFormat> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    format_from_name(&ext).ok_or_else(|| crate::error::BschemaError::UnknownFormat(path.to_path_buf()))
}

/// Maps a format/extension name (`"ttl"`, `"turtle"`, `"nt"`, `"xml"`, ...)
/// to an [`RdfFormat`]. Shared by file-extension detection and the Python
/// bindings, which accept the same names as a `format=` string argument.
pub fn format_from_name(name: &str) -> Option<RdfFormat> {
    match name.to_ascii_lowercase().as_str() {
        "ttl" | "turtle" => Some(RdfFormat::Turtle),
        "nt" | "ntriples" => Some(RdfFormat::NTriples),
        "nq" | "nquads" => Some(RdfFormat::NQuads),
        "rdf" | "xml" | "owl" | "rdfxml" => Some(RdfFormat::RdfXml),
        "trig" => Some(RdfFormat::TriG),
        _ => None,
    }
}
