//! Builds an "example graph": the class graph with each `bs:`-namespaced
//! class node replaced by real members of that class pulled from the
//! member graph. Meant for humans/LLMs skimming a bschema summary who want
//! to see concrete instance names instead of abstract class IRIs, e.g.
//! `bs:AHU_v1 brick:hasPoint bs:Point_v1` becomes
//! `(ex:AHU_1 ex:AHU_2) brick:hasPoint (ex:point_1 ex:point_2) .`
//!
//! Three things this does beyond a literal find-and-replace:
//!
//! - **Prefixes.** Every IRI namespace actually used in the output gets a
//!   `@prefix` binding: the crate's own known short names (`brick:`, `ex:`,
//!   ...) where they apply, else an auto-numbered `ns1:`, `ns2:`, ... -
//!   assigned in sorted-namespace order so it's stable across runs, not in
//!   whatever order the class graph happens to iterate in. Nothing is left
//!   as a long bracketed `<...>` IRI unless its local part genuinely isn't
//!   safe to abbreviate.
//! - **Blank nodes.** A class whose sampled members are *all* blank nodes
//!   (real or, for a `member_graph` computed by an older version, a raw
//!   `urn:bschema-rs:skolem:...` stand-in) collapses to a single bare `[]`
//!   instead of a parenthesized list of meaningless-looking blank node
//!   labels: unlike named instances, several examples of "an anonymous
//!   node" carry no more information than one.
//! - **No synthetic bookkeeping.** `RdfGraph::skolemize` tags every
//!   literal-derived skolem node with `<node> a rdfs:Literal .` purely so
//!   the matching algorithm can treat literals uniformly; that's an
//!   implementation artifact of this crate; it isn't part of the model
//!   and does not belong in a representation meant to be read.
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
use oxigraph::model::{Literal, NamedNode, NamedOrBlankNode, Term, Triple};
use std::collections::{HashMap, HashSet};

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
    let triples: Vec<Triple> = class_graph.triples().into_iter().filter(|t| !is_literal_marker(t)).collect();

    let mut namespaces: HashSet<String> = HashSet::new();
    for t in &triples {
        observe_term(&mut namespaces, &Term::NamedNode(t.predicate.clone()));
        observe_position(&mut namespaces, &Term::from(t.subject.clone()), &members_by_class);
        observe_position(&mut namespaces, &t.object, &members_by_class);
    }
    let prefixes = PrefixTable::build(&namespaces);

    let mut out = prefixes.header();
    for t in &triples {
        let subject = render_position(&Term::from(t.subject.clone()), &members_by_class, &prefixes);
        let object = render_position(&t.object, &members_by_class, &prefixes);
        let predicate = if t.predicate == *namespace::A { "a".to_string() } else { prefixes.abbreviate(t.predicate.as_str()) };
        out.push_str(&format!("{subject} {predicate} {object} .\n"));
    }

    Ok(out)
}

/// `<node> a rdfs:Literal .` triples are synthetic bookkeeping added by
/// `RdfGraph::skolemize` (see module docs), not part of the model.
fn is_literal_marker(t: &Triple) -> bool {
    t.predicate == *namespace::A && matches!(&t.object, Term::NamedNode(n) if n.as_str() == namespace::RDFS_LITERAL.as_str())
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

/// What a subject/object position in a class-pattern triple renders as
/// once `bs:` class substitution is resolved.
enum Position<'a> {
    /// Not a substitutable `bs:` class (or has no members): the term as-is.
    Term(&'a Term),
    /// A substitutable class with at least one non-blank sampled member:
    /// a parenthesized Turtle collection of up to `example_count` members.
    Collection(&'a [Term]),
    /// A substitutable class whose sampled members are all blank nodes:
    /// several indistinguishable-looking anonymous IDs carry no more
    /// information than one, so collapse to a single bare `[]`.
    AnonBlank,
}

fn resolve<'a>(term: &'a Term, members_by_class: &'a HashMap<NamedNode, Vec<Term>>) -> Position<'a> {
    if let Term::NamedNode(n) = term {
        if let Some(members) = members_by_class.get(n) {
            if !members.is_empty() && members.iter().all(is_blank_like) {
                return Position::AnonBlank;
            }
            return Position::Collection(members);
        }
    }
    Position::Term(term)
}

fn is_blank_like(t: &Term) -> bool {
    match t {
        Term::BlankNode(_) => true,
        Term::NamedNode(n) => n.as_str().starts_with(namespace::BNODE_BASE),
        Term::Literal(_) => false,
    }
}

fn observe_position(namespaces: &mut HashSet<String>, term: &Term, members_by_class: &HashMap<NamedNode, Vec<Term>>) {
    match resolve(term, members_by_class) {
        Position::Term(t) => observe_term(namespaces, t),
        Position::Collection(items) => items.iter().for_each(|t| observe_term(namespaces, t)),
        Position::AnonBlank => {}
    }
}

fn observe_term(namespaces: &mut HashSet<String>, term: &Term) {
    match term {
        Term::NamedNode(n) if !n.as_str().starts_with(namespace::BNODE_BASE) => {
            observe_iri(namespaces, n.as_str());
        }
        Term::Literal(l) if l.language().is_none() && l.datatype().as_str() != XSD_STRING => {
            observe_iri(namespaces, l.datatype().as_str());
        }
        _ => {}
    }
}

fn observe_iri(namespaces: &mut HashSet<String>, iri: &str) {
    let (ns, local) = split_namespace(iri);
    if !ns.is_empty() && is_safe_pn_local(local) {
        namespaces.insert(ns.to_string());
    }
}

fn render_position(term: &Term, members_by_class: &HashMap<NamedNode, Vec<Term>>, prefixes: &PrefixTable) -> String {
    match resolve(term, members_by_class) {
        Position::Term(t) => format_term(t, prefixes),
        Position::Collection(items) => {
            format!("({})", items.iter().map(|t| format_term(t, prefixes)).collect::<Vec<_>>().join(" "))
        }
        Position::AnonBlank => "[]".to_string(),
    }
}

fn format_term(term: &Term, prefixes: &PrefixTable) -> String {
    match term {
        // See `Position::AnonBlank`: a lone blank-like term outside of an
        // all-blank group (e.g. one member of an otherwise-named/literal
        // class) still reads better as `[]` than as an opaque skolem IRI
        // or a bare hash-labelled blank node.
        Term::NamedNode(n) if n.as_str().starts_with(namespace::BNODE_BASE) => "[]".to_string(),
        Term::NamedNode(n) => prefixes.abbreviate(n.as_str()),
        Term::BlankNode(_) => "[]".to_string(),
        Term::Literal(l) => format_literal(l, prefixes),
    }
}

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

fn format_literal(l: &Literal, prefixes: &PrefixTable) -> String {
    let value = escape_turtle_string(l.value());
    if let Some(lang) = l.language() {
        format!("\"{value}\"@{lang}")
    } else if l.datatype().as_str() == XSD_STRING {
        format!("\"{value}\"")
    } else {
        format!("\"{value}\"^^{}", prefixes.abbreviate(l.datatype().as_str()))
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

/// Maps each namespace actually used in the output to a `@prefix` name:
/// the crate's own known short names where they apply, else an
/// auto-numbered `ns1:`, `ns2:`, ... assigned in sorted-namespace order
/// (not "order first seen while walking the class graph", which would
/// make the numbering depend on the store's internal triple ordering).
struct PrefixTable {
    by_namespace: HashMap<String, String>,
}

impl PrefixTable {
    fn build(namespaces: &HashSet<String>) -> Self {
        let mut by_namespace = HashMap::new();
        let mut unknown: Vec<&String> = Vec::new();

        for ns in namespaces {
            match namespace::prefix_table().into_iter().find(|(_, base)| base == ns) {
                Some((known, _)) => {
                    by_namespace.insert(ns.clone(), known.to_string());
                }
                None => unknown.push(ns),
            }
        }

        unknown.sort();
        for (i, ns) in unknown.into_iter().enumerate() {
            by_namespace.insert(ns.clone(), format!("ns{}", i + 1));
        }

        Self { by_namespace }
    }

    /// Shortens `iri` to a `prefix:local` form when its namespace has a
    /// binding and the local part is a safe Turtle `PN_LOCAL`; otherwise
    /// falls back to a bracketed full IRI.
    fn abbreviate(&self, iri: &str) -> String {
        let (ns, local) = split_namespace(iri);
        if is_safe_pn_local(local) {
            if let Some(prefix) = self.by_namespace.get(ns) {
                return format!("{prefix}:{local}");
            }
        }
        format!("<{iri}>")
    }

    fn header(&self) -> String {
        let mut entries: Vec<(&String, &String)> = self.by_namespace.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));

        let mut s = String::new();
        for (ns, prefix) in entries {
            s.push_str(&format!("@prefix {prefix}: <{ns}> .\n"));
        }
        if !s.is_empty() {
            s.push('\n');
        }
        s
    }
}

/// Splits `iri` right after its last `#` or `/`, mirroring the usual
/// QName-splitting heuristic. Returns an empty local part (signalling "not
/// safely abbreviatable") if neither separator is present, or one is but
/// nothing follows it.
fn split_namespace(iri: &str) -> (&str, &str) {
    match [iri.rfind('#'), iri.rfind('/')].into_iter().flatten().max() {
        Some(p) if p + 1 < iri.len() => (&iri[..=p], &iri[p + 1..]),
        _ => (iri, ""),
    }
}

fn is_safe_pn_local(s: &str) -> bool {
    !s.is_empty()
        && !s.ends_with('.')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}
