use bschema_core::error::BschemaError;
use bschema_core::graph::RdfGraph;
use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;

fn to_py_err(err: BschemaError) -> PyErr {
    match err {
        BschemaError::Io(e) => PyIOError::new_err(e.to_string()),
        other => PyValueError::new_err(other.to_string()),
    }
}

fn resolve_format(name: &str) -> PyResult<bschema_core::RdfFormat> {
    bschema_core::graph::format_from_name(name)
        .ok_or_else(|| PyValueError::new_err(format!("unrecognized RDF format {name:?}")))
}

#[pymodule]
mod _bschema_rs {
    use super::*;

    /// Runs the bschema summarization algorithm over RDF data given as a string.
    ///
    /// Returns `(class_graph_turtle, member_graph_turtle, iterations)`.
    #[pyfunction]
    #[pyo3(signature = (data, format="turtle", iterations=10, similarity_threshold=None, remove_added_labels=true, use_original_names=true))]
    fn create_bschema(
        data: &str,
        format: &str,
        iterations: usize,
        similarity_threshold: Option<f64>,
        remove_added_labels: bool,
        use_original_names: bool,
    ) -> PyResult<(String, String, usize)> {
        let fmt = resolve_format(format)?;
        let data_graph = RdfGraph::parse_str(data, fmt).map_err(to_py_err)?;
        let result = bschema_core::create_bschema(
            &data_graph,
            iterations,
            similarity_threshold,
            remove_added_labels,
            use_original_names,
        )
        .map_err(to_py_err)?;
        let class_ttl = result.class_graph.serialize_turtle().map_err(to_py_err)?;
        let member_ttl = result.member_graph.serialize_turtle().map_err(to_py_err)?;
        Ok((class_ttl, member_ttl, result.iterations))
    }

    /// Renders a class graph + member graph pair (as produced by
    /// `create_bschema`) into "example" Turtle text: each `bs:` class node
    /// replaced by a parenthesized Turtle-collection of up to
    /// `example_count` of its real members, e.g. `bs:AHU_v1 brick:hasPoint
    /// bs:Point_v1` becomes `(ex:AHU_1 ex:AHU_2) brick:hasPoint (ex:point_1
    /// ex:point_2) .`. Meant for humans/LLMs skimming a summary.
    #[pyfunction]
    #[pyo3(signature = (class_graph, member_graph, example_count=2))]
    fn example_turtle(class_graph: &str, member_graph: &str, example_count: usize) -> PyResult<String> {
        let class_graph = RdfGraph::parse_str(class_graph, bschema_core::RdfFormat::Turtle).map_err(to_py_err)?;
        let member_graph = RdfGraph::parse_str(member_graph, bschema_core::RdfFormat::Turtle).map_err(to_py_err)?;
        bschema_core::examples::example_turtle(&class_graph, &member_graph, example_count).map_err(to_py_err)
    }

    /// Runs the bschema summarization algorithm over an RDF file on disk.
    /// The format is inferred from the file extension (`.ttl`, `.nt`, `.rdf`, ...).
    ///
    /// Returns `(class_graph_turtle, member_graph_turtle, iterations)`.
    #[pyfunction]
    #[pyo3(signature = (path, iterations=10, similarity_threshold=None, remove_added_labels=true, use_original_names=true))]
    fn create_bschema_from_file(
        path: &str,
        iterations: usize,
        similarity_threshold: Option<f64>,
        remove_added_labels: bool,
        use_original_names: bool,
    ) -> PyResult<(String, String, usize)> {
        let data_graph = RdfGraph::parse_file(std::path::Path::new(path)).map_err(to_py_err)?;
        let result = bschema_core::create_bschema(
            &data_graph,
            iterations,
            similarity_threshold,
            remove_added_labels,
            use_original_names,
        )
        .map_err(to_py_err)?;
        let class_ttl = result.class_graph.serialize_turtle().map_err(to_py_err)?;
        let member_ttl = result.member_graph.serialize_turtle().map_err(to_py_err)?;
        Ok((class_ttl, member_ttl, result.iterations))
    }
}
