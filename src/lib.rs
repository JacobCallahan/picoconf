use pyo3::prelude::*;

mod picoconf;

#[pymodule]
fn _picoconf(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<picoconf::PicoConf>()?;
    Ok(())
}
