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

pub type CanonTriple = (String, String, String);
type Color = u64;

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Role {
    Subject,
    Object,
}

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
fn refine(
    triples: &[Triple],
    colors: &HashMap<Term, Color>,
) -> HashMap<Term, Color> {
    let mut signatures: HashMap<BlankNode, Vec<(Role, String, Color)>> = HashMap::new();
    for t in triples {
        if let NamedOrBlankNode::BlankNode(b) = &t.subject {
            let obj_color = colors.get(&t.object).cloned().unwrap_or(0);
            signatures
                .entry(b.clone())
                .or_default()
                .push((Role::Subject, t.predicate.as_str().to_string(), obj_color));
        }
        if let Term::BlankNode(b) = &t.object {
            let subj_color = colors.get(&Term::from(t.subject.clone())).cloned().unwrap_or(0);
            signatures
                .entry(b.clone())
                .or_default()
                .push((Role::Object, t.predicate.as_str().to_string(), subj_color));
        }
    }

    let mut next_colors = colors.clone();
    let mut sig_to_color = HashMap::new();
    let mut color_counter = 1u64;

    for (b, color) in colors {
        if let Term::BlankNode(bn) = b {
            let mut sig = signatures.remove(&bn).unwrap_or_default();
            sig.sort();
            
            let signature = (color, sig);
            let assigned_color = *sig_to_color.entry(signature).or_insert_with(|| {
                let c = color_counter;
                color_counter += 1;
                c
            });
            next_colors.insert(Term::BlankNode(bn.clone()), assigned_color);
        }
    }
    next_colors
}

/// Deterministically canonicalizes `triples` into ground `(s, p, o)` string
/// triples: blank nodes are replaced with structural labels derived from
/// 1-WL color refinement so that two isomorphic graphs (up to blank node
/// relabeling) produce identical canonical sets.
pub fn canonicalize(triples: &[Triple]) -> HashSet<CanonTriple> {
    let mut blank_nodes: HashSet<BlankNode> = HashSet::new();
    let mut ground_terms: HashSet<Term> = HashSet::new();
    for t in triples {
        if let NamedOrBlankNode::BlankNode(b) = &t.subject {
            blank_nodes.insert(b.clone());
        } else {
            ground_terms.insert(Term::from(t.subject.clone()));
        }
        if let Term::BlankNode(b) = &t.object {
            blank_nodes.insert(b.clone());
        } else {
            ground_terms.insert(t.object.clone());
        }
    }

    let mut colors: HashMap<Term, Color> = HashMap::new();
    let mut color_counter = 1u64;
    for term in ground_terms {
        colors.insert(term, color_counter);
        color_counter += 1;
    }
    for b in &blank_nodes {
        colors.insert(Term::BlankNode(b.clone()), 0);
    }

    for _ in 0..=blank_nodes.len() {
        colors = refine(triples, &colors);
    }

    // Rank blank nodes by (final color, first-appearance order) so the
    // label assigned depends only on graph structure, not on the arbitrary
    // internal blank node identifiers.
    let mut order: Vec<&BlankNode> = blank_nodes.iter().collect();
    order.sort_by(|a, b| {
        let ca = colors.get(&Term::BlankNode((*a).clone())).unwrap_or(&0);
        let cb = colors.get(&Term::BlankNode((*b).clone())).unwrap_or(&0);
        ca.cmp(cb).then_with(|| a.as_str().cmp(b.as_str()))
    });
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

/// Number of triples the two canonicalized graphs have in common.
pub fn common_canon_triple_count(a: &HashSet<CanonTriple>, b: &HashSet<CanonTriple>) -> usize {
    a.intersection(b).count()
}
