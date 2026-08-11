//! Builds an "example graph": the class graph with each `bs:`-namespaced
//! class node replaced by real members of that class pulled from the
//! member graph. Meant for humans/LLMs skimming a bschema summary who want
//! to see concrete instance names instead of abstract class IRIs, e.g.
//! `bs:AHU_v1 brick:hasPoint bs:Point_v1` becomes
//! `(ex:AHU_1 ex:AHU_2) brick:hasPoint (ex:point_1 ex:point_2) .`
//!
//! A few things this does beyond a literal find-and-replace:
//!
//! - **Repeated subject+predicate.** A subject class often relates to
//!   several different object classes through the same predicate (e.g.
//!   several `brick:hasPoint` rows for the same subject list); rather than
//!   repeat the subject on its own near-duplicate line each time, these
//!   share one statement via Turtle's object-list comma syntax, with each
//!   object on its own indented continuation line.
//! - **Prefixes.** Every IRI namespace actually used in the output gets a
//!   `@prefix` binding: the crate's own known short names (`brick:`, `ex:`,
//!   ...) where they apply, else an auto-numbered `ns1:`, `ns2:`, ... -
//!   assigned in sorted-namespace order so it's stable across runs, not in
//!   whatever order the class graph happens to iterate in. Nothing is left
//!   as a long bracketed `<...>` IRI unless its local part genuinely isn't
//!   safe to abbreviate.
//! - **Blank nodes.** A class whose sampled members are *all* blank nodes
//!   has no meaningful name to substitute (unlike a named instance, several
//!   examples of "an anonymous node" carry no more information than one),
//!   so instead of listing members at all, this shows what that class
//!   itself asserts: its own class-pattern triples, nested inline as a
//!   real Turtle blank-node property list, e.g. `ref:hasExternalReference
//!   [ ref:hasTimeseriesId (...) ]` rather than a bare `[]` or a list of
//!   meaningless hash-labelled blank node IDs. A blank class that's never
//!   referenced as an object anywhere still needs to be shown somewhere,
//!   so it's emitted as its own top-level `[ ... ] .` statement.
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

/// A class-pattern triple's `(predicate, object)` pairs, keyed by subject
/// class - i.e. what that class itself asserts, per `class_graph`.
type ClassProperties = HashMap<NamedNode, Vec<(NamedNode, Term)>>;

/// Renders the example representation described at module level from an
/// already computed `class_graph` and `member_graph` (as returned by
/// [`crate::create_bschema`]) as ready-to-write Turtle text.
pub fn example_turtle(class_graph: &RdfGraph, member_graph: &RdfGraph, example_count: usize) -> Result<String> {
    let members_by_class = collect_members(member_graph, example_count);
    let triples: Vec<Triple> = class_graph.triples().into_iter().filter(|t| !is_literal_marker(t)).collect();

    // Triples whose subject is an all-blank class are never shown as their
    // own top-level line: they're consumed as a nested `[ ... ]` wherever
    // that class is referenced (or, failing that, as a standalone root
    // statement once every other triple has been rendered).
    let mut blank_class_properties: ClassProperties = HashMap::new();
    let mut top_level: Vec<&Triple> = Vec::new();
    for t in &triples {
        match &t.subject {
            NamedOrBlankNode::NamedNode(n) if is_blank_class(n, &members_by_class) => {
                blank_class_properties.entry(n.clone()).or_default().push((t.predicate.clone(), t.object.clone()));
            }
            _ => top_level.push(t),
        }
    }
    for props in blank_class_properties.values_mut() {
        props.sort_by(|a, b| (a.0.as_str(), a.1.to_string()).cmp(&(b.0.as_str(), b.1.to_string())));
    }

    let mut namespaces: HashSet<String> = HashSet::new();
    let mut visiting: HashSet<NamedNode> = HashSet::new();
    for t in &top_level {
        observe_term(&mut namespaces, &Term::NamedNode(t.predicate.clone()));
        observe_position(&mut namespaces, &Term::from(t.subject.clone()), &members_by_class, &blank_class_properties, &mut visiting);
        observe_position(&mut namespaces, &t.object, &members_by_class, &blank_class_properties, &mut visiting);
    }
    for class in blank_class_properties.keys() {
        observe_blank_class(&mut namespaces, class, &members_by_class, &blank_class_properties, &mut visiting);
    }
    let prefixes = PrefixTable::build(&namespaces);

    let mut out = prefixes.header();
    let mut referenced: HashSet<NamedNode> = HashSet::new();
    visiting.clear();

    // Group by (subject, predicate): a subject class often relates to
    // several different object classes through the same predicate (e.g.
    // several `brick:hasPoint` rows), which otherwise means several
    // near-duplicate top-level lines repeating the same subject list. Turtle's
    // standard object-list comma syntax says that once.
    let mut grouped: HashMap<(NamedOrBlankNode, NamedNode), Vec<Term>> = HashMap::new();
    for t in &top_level {
        grouped.entry((t.subject.clone(), t.predicate.clone())).or_default().push(t.object.clone());
    }
    let mut group_keys: Vec<(NamedOrBlankNode, NamedNode)> = grouped.keys().cloned().collect();
    group_keys.sort_by_key(|(s, p)| (Term::from(s.clone()).to_string(), p.as_str().to_string()));

    for key in &group_keys {
        let (subject, predicate) = key;
        let objects = &grouped[key];
        let subject_text =
            render_position(&Term::from(subject.clone()), &members_by_class, &blank_class_properties, &prefixes, &mut referenced, &mut visiting, 0);
        let predicate_text = if *predicate == *namespace::A { "a".to_string() } else { prefixes.abbreviate(predicate.as_str()) };

        if let [object] = objects.as_slice() {
            let object_text = render_position(object, &members_by_class, &blank_class_properties, &prefixes, &mut referenced, &mut visiting, 0);
            out.push_str(&format!("{subject_text} {predicate_text} {object_text} .\n"));
        } else {
            let object_lines: Vec<String> = objects
                .iter()
                .map(|o| format!("    {}", render_position(o, &members_by_class, &blank_class_properties, &prefixes, &mut referenced, &mut visiting, 1)))
                .collect();
            out.push_str(&format!("{subject_text} {predicate_text}\n{} .\n", object_lines.join(" ,\n")));
        }
    }

    // Blank classes no top-level triple ever pointed to would otherwise be
    // silently dropped - surface them as their own root statement, in a
    // fixed (sorted) order so output stays deterministic. A candidate may
    // get swept up as a nested reference *during* this very loop (one root
    // blank class can point to another), so re-check `referenced` each time
    // rather than trusting a snapshot taken before rendering began.
    let mut candidates: Vec<NamedNode> = blank_class_properties.keys().cloned().collect();
    candidates.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    for class in candidates {
        if referenced.contains(&class) {
            continue;
        }
        let block = render_blank_class(&class, &members_by_class, &blank_class_properties, &prefixes, &mut referenced, &mut visiting, 0);
        out.push_str(&format!("{block} .\n"));
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

/// A class is "all-blank" when it has at least one sampled member and every
/// sampled member is blank-like - real or (for a `member_graph` computed by
/// an older version) a raw skolem stand-in.
fn is_blank_class(class: &NamedNode, members_by_class: &HashMap<NamedNode, Vec<Term>>) -> bool {
    members_by_class.get(class).is_some_and(|members| !members.is_empty() && members.iter().all(is_blank_like))
}

fn is_blank_like(t: &Term) -> bool {
    match t {
        Term::BlankNode(_) => true,
        Term::NamedNode(n) => n.as_str().starts_with(namespace::BNODE_BASE),
        Term::Literal(_) => false,
    }
}

fn observe_position(
    namespaces: &mut HashSet<String>,
    term: &Term,
    members_by_class: &HashMap<NamedNode, Vec<Term>>,
    blank_class_properties: &ClassProperties,
    visiting: &mut HashSet<NamedNode>,
) {
    if let Term::NamedNode(n) = term {
        if is_blank_class(n, members_by_class) {
            observe_blank_class(namespaces, n, members_by_class, blank_class_properties, visiting);
            return;
        }
        if let Some(members) = members_by_class.get(n) {
            members.iter().for_each(|t| observe_term(namespaces, t));
            return;
        }
    }
    observe_term(namespaces, term);
}

fn observe_blank_class(
    namespaces: &mut HashSet<String>,
    class: &NamedNode,
    members_by_class: &HashMap<NamedNode, Vec<Term>>,
    blank_class_properties: &ClassProperties,
    visiting: &mut HashSet<NamedNode>,
) {
    if visiting.contains(class) {
        return; // cycle guard
    }
    visiting.insert(class.clone());
    if let Some(props) = blank_class_properties.get(class) {
        for (p, o) in props {
            observe_term(namespaces, &Term::NamedNode(p.clone()));
            observe_position(namespaces, o, members_by_class, blank_class_properties, visiting);
        }
    }
    visiting.remove(class);
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

fn render_position(
    term: &Term,
    members_by_class: &HashMap<NamedNode, Vec<Term>>,
    blank_class_properties: &ClassProperties,
    prefixes: &PrefixTable,
    referenced: &mut HashSet<NamedNode>,
    visiting: &mut HashSet<NamedNode>,
    indent: usize,
) -> String {
    if let Term::NamedNode(n) = term {
        if is_blank_class(n, members_by_class) {
            return render_blank_class(n, members_by_class, blank_class_properties, prefixes, referenced, visiting, indent);
        }
        if let Some(members) = members_by_class.get(n) {
            return format!("({})", members.iter().map(|t| format_term(t, prefixes)).collect::<Vec<_>>().join(" "));
        }
    }
    format_term(term, prefixes)
}

/// Renders `class`'s own class-pattern triples as a Turtle blank-node
/// property list, e.g. `[\n    ref:hasTimeseriesId (...)\n]`. Falls back to
/// a bare `[]` if nothing is known about it (e.g. a `member_graph` from an
/// older version, reloaded standalone with no matching `class_graph` entry
/// describing that class) or a cycle is detected.
fn render_blank_class(
    class: &NamedNode,
    members_by_class: &HashMap<NamedNode, Vec<Term>>,
    blank_class_properties: &ClassProperties,
    prefixes: &PrefixTable,
    referenced: &mut HashSet<NamedNode>,
    visiting: &mut HashSet<NamedNode>,
    indent: usize,
) -> String {
    referenced.insert(class.clone());

    if visiting.contains(class) {
        return "[]".to_string(); // cycle guard: don't recurse forever
    }
    let Some(props) = blank_class_properties.get(class).filter(|p| !p.is_empty()) else {
        return "[]".to_string();
    };

    visiting.insert(class.clone());
    let inner_indent = "    ".repeat(indent + 1);
    let closing_indent = "    ".repeat(indent);
    let lines: Vec<String> = props
        .iter()
        .map(|(p, o)| {
            let predicate = if *p == *namespace::A { "a".to_string() } else { prefixes.abbreviate(p.as_str()) };
            let object = render_position(o, members_by_class, blank_class_properties, prefixes, referenced, visiting, indent + 1);
            format!("{inner_indent}{predicate} {object}")
        })
        .collect();
    visiting.remove(class);

    format!("[\n{}\n{closing_indent}]", lines.join(" ;\n"))
}

fn format_term(term: &Term, prefixes: &PrefixTable) -> String {
    match term {
        // A lone blank-like term reached outside of `render_blank_class`
        // (e.g. one member of an otherwise-named/literal class that
        // happens to include a stray blank one) still reads better as `[]`
        // than as an opaque skolem IRI or a bare hash-labelled blank node.
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
