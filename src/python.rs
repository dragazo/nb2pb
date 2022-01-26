use pyo3::{prelude::*, exceptions::PyRuntimeError};

#[pyfunction]
fn translate(xml: &str) -> PyResult<(String, String)> {
    crate::translate(xml).map_err(|e| PyRuntimeError::new_err(format!("{:?}", e)))
}

#[pymodule]
fn nb2pb(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(translate, m)?)?;
    Ok(())
}
