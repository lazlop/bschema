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
