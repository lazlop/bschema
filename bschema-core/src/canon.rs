//! Blank-node-aware graph canonicalization, replacing `rdflib.compare`'s
//! `isomorphic()` / `graph_diff()` (rdflib delegates those to its own
//! canonical bnode labeling, then compares/intersects the results).
//!
//! bschema's class-pattern graphs are built from `get_class`, which only
//! ever returns class IRIs — never a blank node — so in practice these
//! graphs are always ground and this reduces to plain set comparison.
//! The 1-WL color-refinement pass below exists so the comparison stays
//! correct if that invariant is ever relaxed (e.g. comparing subgraphs
//! before skolemization), at the cost of not resolving graphs with
//! nontrivial blank-node automorphisms (rare for the ~1-hop patterns
//! bschema compares).

use oxigraph::model::{BlankNode, NamedOrBlankNode, Term, Triple};
use std::collections::{HashMap, HashSet};

type CanonTriple = (String, String, String);

fn ground_key(term: &Term) -> String {
    match term {
        Term::NamedNode(n) => format!("N<{}>", n.as_str()),
        Term::Literal(l) => match l.language() {
            Some(lang) => format!("L\"{}\"@{}", l.value(), lang),
            None => format!("L\"{}\"^^<{}>", l.value(), l.datatype().as_str()),
        },
        Term::BlankNode(_) => unreachable!("blank nodes are canonicalized separately"),
    }
}

fn subject_key(subject: &NamedOrBlankNode) -> Option<String> {
    match subject {
        NamedOrBlankNode::NamedNode(n) => Some(format!("N<{}>", n.as_str())),
        NamedOrBlankNode::BlankNode(_) => None,
    }
}

/// One round of 1-WL color refinement: every blank node's new color folds
/// in its old color plus the sorted multiset of (role, predicate, neighbor
/// color) it participates in.
fn refine(triples: &[Triple], colors: &HashMap<BlankNode, String>) -> HashMap<BlankNode, String> {
    let color_of_subject = |s: &NamedOrBlankNode| -> String {
        match s {
            NamedOrBlankNode::NamedNode(n) => format!("N<{}>", n.as_str()),
            NamedOrBlankNode::BlankNode(b) => colors.get(b).cloned().unwrap_or_default(),
        }
    };
    let color_of_object = |t: &Term| -> String {
        match t {
            Term::BlankNode(b) => colors.get(b).cloned().unwrap_or_default(),
            other => ground_key(other),
        }
    };

    let mut signatures: HashMap<BlankNode, Vec<String>> = HashMap::new();
    for t in triples {
        if let NamedOrBlankNode::BlankNode(b) = &t.subject {
            signatures
                .entry(b.clone())
                .or_default()
                .push(format!("S:{}:{}", t.predicate.as_str(), color_of_object(&t.object)));
        }
        if let Term::BlankNode(b) = &t.object {
            signatures
                .entry(b.clone())
                .or_default()
                .push(format!("O:{}:{}", t.predicate.as_str(), color_of_subject(&t.subject)));
        }
    }

    let mut new_colors = HashMap::new();
    for (b, old_color) in colors {
        let mut sig = signatures.remove(b).unwrap_or_default();
        sig.sort();
        new_colors.insert(b.clone(), format!("{old_color}|{sig:?}"));
    }
    new_colors
}

/// Deterministically canonicalizes `triples` into ground `(s, p, o)` string
/// triples: blank nodes are replaced with structural labels derived from
/// 1-WL color refinement so that two isomorphic graphs (up to blank node
/// relabeling) produce identical canonical sets.
fn canonicalize(triples: &[Triple]) -> HashSet<CanonTriple> {
    let mut blank_nodes: HashSet<BlankNode> = HashSet::new();
    for t in triples {
        if let NamedOrBlankNode::BlankNode(b) = &t.subject {
            blank_nodes.insert(b.clone());
        }
        if let Term::BlankNode(b) = &t.object {
            blank_nodes.insert(b.clone());
        }
    }

    let mut colors: HashMap<BlankNode, String> =
        blank_nodes.iter().map(|b| (b.clone(), "B".to_string())).collect();
    for _ in 0..=blank_nodes.len() {
        colors = refine(triples, &colors);
    }

    // Rank blank nodes by (final color, first-appearance order) so the
    // label assigned depends only on graph structure, not on the arbitrary
    // internal blank node identifiers.
    let mut order: Vec<&BlankNode> = blank_nodes.iter().collect();
    order.sort_by(|a, b| colors[*a].cmp(&colors[*b]).then_with(|| a.as_str().cmp(b.as_str())));
    let labels: HashMap<BlankNode, String> = order
        .into_iter()
        .enumerate()
        .map(|(i, b)| (b.clone(), format!("_:c{i}")))
        .collect();

    triples
        .iter()
        .map(|t| {
            let s = subject_key(&t.subject)
                .unwrap_or_else(|| match &t.subject {
                    NamedOrBlankNode::BlankNode(b) => labels[b].clone(),
                    NamedOrBlankNode::NamedNode(_) => unreachable!(),
                });
            let p = format!("N<{}>", t.predicate.as_str());
            let o = match &t.object {
                Term::BlankNode(b) => labels[b].clone(),
                other => ground_key(other),
            };
            (s, p, o)
        })
        .collect()
}

/// Equivalent to `rdflib.compare.isomorphic(a, b)`.
pub fn is_isomorphic(a: &[Triple], b: &[Triple]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    canonicalize(a) == canonicalize(b)
}

/// Number of triples the two graphs have in common after canonicalization,
/// i.e. `len(graph_diff(a, b)[0])` from `rdflib.compare.graph_diff`.
pub fn common_triple_count(a: &[Triple], b: &[Triple]) -> usize {
    let ca = canonicalize(a);
    let cb = canonicalize(b);
    ca.intersection(&cb).count()
}
