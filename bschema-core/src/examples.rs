//! Builds an "example graph": the class graph with each `bs:`-namespaced
//! class node replaced by an RDF collection (Turtle `( ... )` list syntax)
//! of up to `example_count` real members of that class, pulled from the
//! member graph. Meant for humans/LLMs skimming a bschema summary who want
//! to see concrete instance names instead of abstract class IRIs, e.g.
//! `bs:AHU_v1 brick:hasPoint bs:Point_v1` becomes
//! `(ex:AHU_1 ex:AHU_2) brick:hasPoint (ex:point_1 ex:point_2) .`
//!
//! The two lists are independently sampled per class (the same members
//! every time that class appears), not aligned real-world edges: list
//! position `i` on one side is not claimed to correspond to position `i`
//! on the other.
//!
//! This renders straight to a Turtle-syntax string rather than building
//! real `rdf:first`/`rdf:rest` triples in an [`RdfGraph`]: generic Turtle
//! writers (Oxigraph's own serializer, and rdflib's on the Python side)
//! only fold a collection back into `( ... )` syntax when its head is used
//! purely as an object, never when the head itself is the *subject* of the
//! enclosing triple - which is exactly what happens here on the left-hand
//! side of every relabeled triple. Writing the text directly sidesteps
//! that limitation and guarantees the intended compact form regardless of
//! how the result is consumed.
use crate::algorithm::BschemaResult;
use crate::error::Result;
use crate::graph::RdfGraph;
use crate::namespace::{self, RDFS_MEMBER};
use oxigraph::model::{Literal, NamedNode, NamedOrBlankNode, Term};
use std::collections::HashMap;

impl BschemaResult {
    /// See [`example_turtle`].
    pub fn example_turtle(&self, example_count: usize) -> Result<String> {
        example_turtle(&self.class_graph, &self.member_graph, example_count)
    }
}

/// Renders the example representation described at module level from an
/// already computed `class_graph` and `member_graph` (as returned by
/// [`crate::create_bschema`]) as ready-to-write Turtle text.
pub fn example_turtle(class_graph: &RdfGraph, member_graph: &RdfGraph, example_count: usize) -> Result<String> {
    let members_by_class = collect_members(member_graph, example_count);
    let mut out = crate::graph::prefix_header();

    for t in class_graph.triples() {
        let subject = match &t.subject {
            NamedOrBlankNode::NamedNode(n) => members_by_class.get(n).map(|members| format_collection(members)),
            NamedOrBlankNode::BlankNode(_) => None,
        }
        .unwrap_or_else(|| format_term(&Term::from(t.subject.clone())));

        let object = match &t.object {
            Term::NamedNode(n) => members_by_class.get(n).map(|members| format_collection(members)),
            _ => None,
        }
        .unwrap_or_else(|| format_term(&t.object));

        let predicate = if t.predicate == *namespace::A {
            "a".to_string()
        } else {
            abbreviate_iri(t.predicate.as_str())
        };
        out.push_str(&format!("{subject} {predicate} {object} .\n"));
    }

    Ok(out)
}

/// Groups `member_graph`'s `rdfs:member` triples by class, sorted by each
/// member's canonical string form for determinism, capped at `example_count`.
fn collect_members(member_graph: &RdfGraph, example_count: usize) -> HashMap<NamedNode, Vec<Term>> {
    let mut by_class: HashMap<NamedNode, Vec<Term>> = HashMap::new();

    for t in member_graph.triples() {
        if t.predicate != *RDFS_MEMBER {
            continue;
        }
        if let NamedOrBlankNode::NamedNode(class) = &t.subject {
            by_class.entry(class.clone()).or_default().push(t.object);
        }
    }

    for members in by_class.values_mut() {
        members.sort_by_key(|t| t.to_string());
        members.truncate(example_count);
    }

    by_class
}

fn format_collection(items: &[Term]) -> String {
    format!("({})", items.iter().map(format_term).collect::<Vec<_>>().join(" "))
}

fn format_term(term: &Term) -> String {
    match term {
        Term::NamedNode(n) => abbreviate_iri(n.as_str()),
        Term::BlankNode(b) => format!("_:{}", b.as_str()),
        Term::Literal(l) => format_literal(l),
    }
}

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

fn format_literal(l: &Literal) -> String {
    let value = escape_turtle_string(l.value());
    if let Some(lang) = l.language() {
        format!("\"{value}\"@{lang}")
    } else if l.datatype().as_str() == XSD_STRING {
        format!("\"{value}\"")
    } else {
        format!("\"{value}\"^^{}", abbreviate_iri(l.datatype().as_str()))
    }
}

fn escape_turtle_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Shortens `iri` to a `prefix:local` form using [`namespace::prefix_table`]
/// when it falls under a known namespace and the local part is a safe
/// Turtle `PN_LOCAL`; otherwise falls back to a bracketed full IRI.
fn abbreviate_iri(iri: &str) -> String {
    for (prefix, base) in namespace::prefix_table() {
        if let Some(local) = iri.strip_prefix(base) {
            if is_safe_pn_local(local) {
                return format!("{prefix}:{local}");
            }
        }
    }
    format!("<{iri}>")
}

fn is_safe_pn_local(s: &str) -> bool {
    !s.is_empty()
        && !s.ends_with('.')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}
