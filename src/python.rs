use pyo3::{prelude::*, exceptions::PyRuntimeError};

#[pyfunction]
fn translate(xml: &str) -> PyResult<(String, String)> {
    match crate::translate(xml) {
        Ok((a, b)) => Ok((a.to_string(), b.to_string())),
        Err(e) => Err(PyRuntimeError::new_err(format!("{:?}", e))),
    }
}

#[pymodule]
fn nb2pb(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(translate, m)?)?;
    Ok(())
}
