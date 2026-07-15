use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};

fn register_submodule(
    py: Python<'_>,
    parent: &Bound<'_, PyModule>,
    name: &str,
    register: impl FnOnce(&Bound<'_, PyModule>) -> PyResult<()>,
) -> PyResult<()> {
    let submodule = PyModule::new(py, name)?;
    register(&submodule)?;
    parent.add_submodule(&submodule)?;
    let full_name = format!("bacup_lib._native.{name}");
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    let sys_modules = modules.cast::<PyDict>()?;
    sys_modules.set_item(full_name, &submodule)?;
    Ok(())
}

#[pymodule]
fn _native(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    register_submodule(
        py,
        m,
        "conversion_native",
        conversion_native::register_module,
    )?;
    register_submodule(
        py,
        m,
        "esp_authoring_core",
        esp_authoring_core::register_module,
    )?;
    Ok(())
}
