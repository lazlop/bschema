//! Core bschema algorithm, ported from `graph-pattern-id/bschema/bschema.py`.

use crate::canon::{canonicalize, common_canon_triple_count, named_node_key, CanonTriple};
use crate::error::Result;
use crate::graph::RdfGraph;
use crate::namespace::{
    self, ex_ontology_subject, named_node_predicates, A, OWL_ONTOLOGY, RDFS_LITERAL,
    RDFS_RESOURCE, RDF_SEQ, RDFS_MEMBER,
};
use crate::util::{common_pattern, local_name};
use oxigraph::model::{NamedNode, NamedOrBlankNode, Term, Triple};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

/// Extracts the subgraph within `num_hops` of `central_node`, including
/// `rdf:type` triples for every entity encountered. Ports
/// `get_subgraph_with_hops`.
pub fn subgraph_with_hops(
    graph: &RdfGraph,
    central_node: &NamedOrBlankNode,
    num_hops: usize,
    get_classes: bool,
) -> Result<RdfGraph> {
    let subgraph = RdfGraph::new()?;
    let mut visited: HashSet<NamedOrBlankNode> = HashSet::new();
    let mut current_layer: HashSet<NamedOrBlankNode> = HashSet::from([central_node.clone()]);

    for class_uri in graph.objects(central_node, &A) {
        subgraph.insert(&Triple::new(central_node.clone(), A.clone(), class_uri));
    }

    for _ in 0..num_hops {
        let mut next_layer: HashSet<NamedOrBlankNode> = HashSet::new();

        for node in &current_layer {
            if visited.contains(node) {
                continue;
            }
            visited.insert(node.clone());

            for (p, o) in graph.predicate_objects(node) {
                subgraph.insert(&Triple::new(node.clone(), p, o.clone()));
                if let Term::NamedNode(o_named) = &o {
                    let o_subj = NamedOrBlankNode::NamedNode(o_named.clone());
                    next_layer.insert(o_subj.clone());
                    for class_uri in graph.objects(&o_subj, &A) {
                        subgraph.insert(&Triple::new(o_subj.clone(), A.clone(), class_uri));
                    }
                }
            }

            for (s, p) in graph.subject_predicates(&Term::from(node.clone())) {
                subgraph.insert(&Triple::new(s.clone(), p, Term::from(node.clone())));
                if let NamedOrBlankNode::NamedNode(_) = &s {
                    next_layer.insert(s.clone());
                    for class_uri in graph.objects(&s, &A) {
                        subgraph.insert(&Triple::new(s.clone(), A.clone(), class_uri));
                    }
                }
            }
        }

        current_layer = next_layer;
    }

    if get_classes {
        for t in subgraph.triples() {
            for class_uri in graph.objects(&t.subject, &A) {
                subgraph.insert(&Triple::new(t.subject.clone(), A.clone(), class_uri));
            }
            if let Term::NamedNode(o_named) = &t.object {
                let o_subj = NamedOrBlankNode::NamedNode(o_named.clone());
                for class_uri in graph.objects(&o_subj, &A) {
                    subgraph.insert(&Triple::new(o_subj.clone(), A.clone(), class_uri));
                }
            }
        }
    }

    Ok(subgraph)
}

/// Gets the class of a node, preferring a `bs:`-namespaced class if present,
/// else the first `rdf:type` found, else a default for literals/resources.
/// Ports `get_class`.
pub fn get_class(node: &Term, data_graph: &RdfGraph) -> NamedNode {
    let subject = match node {
        Term::NamedNode(n) => Some(NamedOrBlankNode::NamedNode(n.clone())),
        Term::BlankNode(b) => Some(NamedOrBlankNode::BlankNode(b.clone())),
        Term::Literal(_) => None,
    };

    if let Some(subject) = subject {
        let types = data_graph.objects(&subject, &A);
        if let Some(Term::NamedNode(bs_class)) = types
            .iter()
            .find(|o| matches!(o, Term::NamedNode(n) if n.as_str().starts_with(namespace::BS_BASE)))
        {
            return bs_class.clone();
        }
        if let Some(Term::NamedNode(first)) = types.first() {
            return first.clone();
        }
    }

    match node {
        Term::Literal(_) => RDFS_LITERAL.clone(),
        _ => RDFS_RESOURCE.clone(),
    }
}

/// Builds the class-level pattern `(class(s), p, class(o))` for a triple,
/// treating a fixed set of "named node" predicates (units, quantity kinds,
/// aspects, ...) as already denoting the class of their object. Ports
/// `create_class_pattern`.
pub fn class_pattern(triple: &Triple, data_graph: &RdfGraph) -> Triple {
    let s_class = get_class(&Term::from(triple.subject.clone()), data_graph);

    let o_class = if triple.predicate == *A {
        match &triple.object {
            Term::NamedNode(n) => n.clone(),
            other => get_class(other, data_graph),
        }
    } else if named_node_predicates().iter().any(|p| **p == triple.predicate) {
        match &triple.object {
            Term::NamedNode(n) => n.clone(),
            other => get_class(other, data_graph),
        }
    } else {
        get_class(&triple.object, data_graph)
    };

    Triple::new(s_class, triple.predicate.clone(), o_class)
}

/// Ports `create_class_graph`: the deduplicated set of class-patterns for
/// every triple in `data_graph`.
pub fn class_graph(data_graph: &RdfGraph) -> Result<RdfGraph> {
    let out = RdfGraph::new()?;
    for t in data_graph.triples() {
        let pattern = class_pattern(&t, data_graph);
        if !out.contains(&pattern) {
            out.insert(&pattern);
        }
    }
    Ok(out)
}

pub struct ClassIsomorphisms {
    pub distinct_class_subgraphs: Vec<HashSet<CanonTriple>>,
    pub equivalent_subjects: Vec<Vec<NamedOrBlankNode>>,
    pub subject_classes: Vec<NamedNode>,
}

/// Is `t` the canonical form of `<literal-skolem> a rdfs:Literal`, the
/// synthetic marker `RdfGraph::skolemize` tags every not-yet-classified
/// literal skolem with? It's identical for every such literal regardless
/// of datatype, predicate, or subject, so it carries zero discriminating
/// power - yet it appears in the 1-hop pattern of anything that mentions
/// one (the literal's own pattern, and any entity's pattern that has an
/// edge to it). Because those patterns are small (a literal's own is
/// typically 2-3 triples total), this one always-shared triple alone can
/// push an unrelated pair's overlap ratio above a `similarity_threshold`
/// that would otherwise correctly keep them apart - see the over-merging
/// discussion on PR #1 and the follow-up literal-topology work. Filtered
/// out of every pattern in [`class_isomorphisms`] before it's ever stored
/// or compared, for both exact and threshold-based matching.
fn is_literal_marker_canon_triple(t: &CanonTriple) -> bool {
    let literal_key = named_node_key(RDFS_LITERAL.as_str());
    let type_key = named_node_key(A.as_str());
    t.0 == literal_key && t.1 == type_key && t.2 == literal_key
}

/// Is `t` the literal-form (as opposed to [`is_literal_marker_canon_triple`]'s
/// canonicalized form) of `<literal-skolem> a rdfs:Literal`? Never present
/// in the original data graph - purely bookkeeping `RdfGraph::skolemize`
/// adds so the matching algorithm can treat literals uniformly - so it's
/// stripped from `class_graph` in [`create_bschema`] regardless of
/// `remove_added_labels` (which only concerns the `bs:` labels themselves).
pub(crate) fn is_literal_marker(t: &Triple) -> bool {
    t.predicate == *A && matches!(&t.object, Term::NamedNode(n) if n.as_str() == RDFS_LITERAL.as_str())
}

/// Groups subjects of `data_graph` by the isomorphism (or, if
/// `similarity_threshold` is set, high overlap) of their 1-hop class
/// pattern subgraph. Ports `get_class_isomorphisms`.
pub fn class_isomorphisms(
    data_graph: &RdfGraph,
    similarity_threshold: Option<f64>,
) -> Result<ClassIsomorphisms> {
    let subjects_set: HashSet<NamedOrBlankNode> = data_graph.triples().into_iter().map(|t| t.subject.clone()).collect();
    let mut subjects: Vec<NamedOrBlankNode> = subjects_set.into_iter().collect();
    subjects.sort_by(|a, b| node_key(a).cmp(&node_key(b)));

    let results: Vec<Result<(NamedOrBlankNode, NamedNode, HashSet<CanonTriple>)>> = subjects
        .par_iter()
        .map(|s| {
            let subject_class = get_class(&Term::from(s.clone()), data_graph);
            let subgraph = subgraph_with_hops(data_graph, s, 1, false)?;
            let pattern_graph = class_graph(&subgraph)?.triples();
            let canon_pattern: HashSet<CanonTriple> = canonicalize(&pattern_graph)
                .into_iter()
                .filter(|t| !is_literal_marker_canon_triple(t))
                .collect();
            Ok((s.clone(), subject_class, canon_pattern))
        })
        .collect();

    let mut distinct_class_subgraphs: Vec<HashSet<CanonTriple>> = Vec::new();
    let mut equivalent_subjects: Vec<Vec<NamedOrBlankNode>> = Vec::new();
    let mut subject_classes: Vec<NamedNode> = Vec::new();

    for res in results {
        let (s, subject_class, canon_pattern) = res?;

        if equivalent_subjects.is_empty() {
            equivalent_subjects.push(vec![s.clone()]);
            distinct_class_subgraphs.push(canon_pattern);
            subject_classes.push(subject_class);
            continue;
        }

        let indices: Vec<usize> = subject_classes
            .iter()
            .enumerate()
            .filter(|(_, c)| **c == subject_class)
            .map(|(i, _)| i)
            .collect();

        // KNOWN CAVEAT (see PR #1): at threshold=0.0 this only requires
        // intersection > 0 - one single shared pattern triple. Now that
        // literals participate in this same matching (see
        // `RdfGraph::skolemize`), many instances of a type end up sharing
        // at least one "resolves to the same derived literal class" triple
        // (e.g. via a common predicate like hasValue), which alone can
        // satisfy this bound and merge subjects that otherwise share
        // nothing - observed on real data to conflate genuinely distinct
        // physical-quantity classes. Not addressed here by design: the
        // fix should not vary matching behavior by threshold value.
        let mut found = false;
        for i in indices {
            let existing = &distinct_class_subgraphs[i];
            let matched = if let Some(threshold) = similarity_threshold {
                let intersection = common_canon_triple_count(&canon_pattern, existing);
                let smaller = canon_pattern.len().min(existing.len()).max(1);
                (intersection as f64 / smaller as f64) > threshold
            } else {
                canon_pattern == *existing
            };

            if matched {
                equivalent_subjects[i].push(s.clone());
                found = true;
                break;
            }
        }

        if !found {
            distinct_class_subgraphs.push(canon_pattern);
            equivalent_subjects.push(vec![s.clone()]);
            subject_classes.push(subject_class);
        }
    }

    Ok(ClassIsomorphisms { distinct_class_subgraphs, equivalent_subjects, subject_classes })
}

/// Debugging helper: pairs of sublists across two iterations with high
/// overlap (overlap coefficient) that aren't identical. Ports
/// `find_similar_sublists`.
pub fn find_similar_sublists(
    list1: &[Vec<NamedOrBlankNode>],
    list2: &[Vec<NamedOrBlankNode>],
    min_intersection_ratio: f64,
) -> Vec<(usize, usize, f64)> {
    let mut pairs = Vec::new();
    for (i, sub1) in list1.iter().enumerate() {
        let set1: HashSet<&NamedOrBlankNode> = sub1.iter().collect();
        for (j, sub2) in list2.iter().enumerate() {
            let set2: HashSet<&NamedOrBlankNode> = sub2.iter().collect();
            if set1 == set2 {
                continue;
            }
            let intersection = set1.intersection(&set2).count();
            let smaller = set1.len().min(set2.len()).max(1);
            let overlap = intersection as f64 / smaller as f64;
            if overlap >= min_intersection_ratio {
                pairs.push((i, j, overlap));
            }
        }
    }
    pairs
}

/// Ports `lists_have_same_members`: order-independent equality of two
/// lists-of-lists, treated as sets of sets.
pub fn lists_have_same_members(
    list1: &[Vec<NamedOrBlankNode>],
    list2: &[Vec<NamedOrBlankNode>],
) -> bool {
    if list1.len() != list2.len() {
        return false;
    }
    let sets1: HashSet<Vec<NamedOrBlankNode>> = list1.iter().map(|s| sorted(s)).collect();
    let sets2: HashSet<Vec<NamedOrBlankNode>> = list2.iter().map(|s| sorted(s)).collect();
    sets1 == sets2
}

fn sorted(nodes: &[NamedOrBlankNode]) -> Vec<NamedOrBlankNode> {
    let mut v = nodes.to_vec();
    v.sort_by(|a, b| node_key(a).cmp(&node_key(b)));
    v
}

fn node_key(n: &NamedOrBlankNode) -> String {
    match n {
        NamedOrBlankNode::NamedNode(n) => n.as_str().to_string(),
        NamedOrBlankNode::BlankNode(b) => b.as_str().to_string(),
    }
}

/// Assigns a fresh `bs:`-namespaced class name to each equivalence group,
/// deriving the name from the common substring of the group's original
/// subject IRIs (or, if disabled, by versioning the existing class name).
/// Ports `assign_new_classes`.
pub fn assign_new_classes(
    equivalent_subjects: &[Vec<NamedOrBlankNode>],
    subject_classes: &[NamedNode],
    use_original_names: bool,
    counter: &mut HashMap<String, u32>,
) -> HashMap<NamedOrBlankNode, NamedNode> {
    let mut new_subject_classes = HashMap::new();

    for (i, subj_list) in equivalent_subjects.iter().enumerate() {
        let new_cls_name = if use_original_names {
            let iris: Vec<String> = subj_list.iter().map(node_key).collect();
            let mut name = common_pattern(&iris);
            if name.contains(namespace::LITERAL_SKOLEM_BASE) {
                name = "literal".to_string();
            } else if name.contains(namespace::BNODE_BASE) {
                name = "bnode".to_string();
            }
            let count = counter.entry(name.clone()).and_modify(|c| *c += 1).or_insert(1);
            let name_no_uri = local_name(&name);
            namespace::ns(namespace::BS_BASE, &format!("{name_no_uri}{count}"))
        } else {
            let cls_name = &subject_classes[i];
            let local = local_name(cls_name.as_str());
            let name = local.split("_version_").next().unwrap_or(local).to_string();
            let count = counter.entry(name.clone()).and_modify(|c| *c += 1).or_insert(1);
            oxigraph::model::NamedNode::new_unchecked(format!("{name}_version_{count}"))
        };

        for s in subj_list {
            let bs_local = local_name(new_cls_name.as_str());
            new_subject_classes.insert(s.clone(), namespace::ns(namespace::BS_BASE, bs_local));
        }
    }

    new_subject_classes
}

pub struct BschemaResult {
    pub class_graph: RdfGraph,
    pub member_graph: RdfGraph,
    pub iterations: usize,
}

/// Ports `create_bschema`: iteratively summarizes `original_data_graph`
/// into a compact class graph (`H` in the paper) plus a membership graph
/// (`M` in the paper) mapping each derived class back to its members.
pub fn create_bschema(
    original_data_graph: &RdfGraph,
    iterations: usize,
    similarity_threshold: Option<f64>,
    remove_added_labels: bool,
    use_original_names: bool,
) -> Result<BschemaResult> {
    let mut counter: HashMap<String, u32> = HashMap::new();

    original_data_graph.remove(&Triple::new(
        ex_ontology_subject(),
        A.clone(),
        OWL_ONTOLOGY.clone(),
    ));
    let (data_graph, skolem_reverse) = original_data_graph.skolemize()?;

    let mut equivalent_subjects: Vec<Vec<NamedOrBlankNode>> = Vec::new();
    let mut prev_equivalent_subjects: Option<Vec<Vec<NamedOrBlankNode>>> = None;
    let mut prev_subject_classes: Option<HashMap<NamedOrBlankNode, NamedNode>> = None;
    let mut final_iteration = 0;

    for iteration in 0..iterations {
        counter.clear();
        final_iteration = iteration;

        let result = class_isomorphisms(&data_graph, similarity_threshold)?;
        equivalent_subjects = result.equivalent_subjects;
        let subject_classes = result.subject_classes;

        let new_subject_classes =
            assign_new_classes(&equivalent_subjects, &subject_classes, use_original_names, &mut counter);

        if iteration >= 1 {
            if let Some(prev) = &prev_equivalent_subjects {
                if lists_have_same_members(&equivalent_subjects, prev) {
                    break;
                }
            }
        }

        if let Some(prev_classes) = &prev_subject_classes {
            for (s, cls_name) in prev_classes {
                data_graph.remove(&Triple::new(s.clone(), A.clone(), cls_name.clone()));
            }
        }

        for (s, cls_name) in &new_subject_classes {
            data_graph.insert(&Triple::new(s.clone(), A.clone(), cls_name.clone()));
        }

        prev_subject_classes = Some(new_subject_classes);
        prev_equivalent_subjects = Some(equivalent_subjects.clone());

        if similarity_threshold == Some(0.0) {
            break;
        }
    }

    let class_graph_result = class_graph(&data_graph)?;

    if remove_added_labels {
        for t in class_graph_result.triples() {
            if t.predicate == *A {
                if let Term::NamedNode(n) = &t.object {
                    if n.as_str().contains(namespace::BS_BASE) {
                        class_graph_result.remove(&t);
                    }
                }
            }
        }
    }

    // Unlike the bs: labels above (kept or stripped per remove_added_labels),
    // the synthetic literal-skolem marker never corresponds to anything in
    // the original data graph, so it's always stripped from class_graph.
    for t in class_graph_result.triples() {
        if is_literal_marker(&t) {
            class_graph_result.remove(&t);
        }
    }

    // Key the member graph by each group's *applied* class - the label
    // actually inserted into `data_graph` (and thus what `class_graph`
    // above was built from) - not `subject_classes`, which is each group's
    // class as of the *start* of the final iteration (before that
    // iteration's relabeling). Those normally coincide, because the
    // convergence break fires before a would-be-redundant relabeling is
    // applied, leaving `subject_classes` describing the same labels
    // `data_graph` already carries from the previous round. But the
    // `similarity_threshold == 0.0` path breaks immediately *after*
    // applying iteration 0's relabeling, so `subject_classes` there still
    // reflects the *pre*-relabeling classes (e.g. the original `rdf:type`)
    // while `class_graph` reflects the newly applied `bs:` names - keying
    // the member graph by `subject_classes` would silently mismatch the
    // two graphs.
    let applied_classes = prev_subject_classes.unwrap_or_default();
    let member_graph = RdfGraph::new()?;
    for members in &equivalent_subjects {
        let Some(subject_class) = members.first().and_then(|s| applied_classes.get(s)) else {
            continue;
        };
        member_graph.insert(&Triple::new(
            subject_class.clone(),
            A.clone(),
            RDF_SEQ.clone(),
        ));
        for s in members {
            // Report the original literal/blank node, not its skolem
            // stand-in, for members that were skolemized from one.
            let member_term = match s {
                NamedOrBlankNode::NamedNode(n) => {
                    skolem_reverse.get(n).cloned().unwrap_or_else(|| Term::from(s.clone()))
                }
                NamedOrBlankNode::BlankNode(_) => Term::from(s.clone()),
            };
            member_graph.insert(&Triple::new(
                subject_class.clone(),
                RDFS_MEMBER.clone(),
                member_term,
            ));
        }
    }

    Ok(BschemaResult { class_graph: class_graph_result, member_graph, iterations: final_iteration })
}
