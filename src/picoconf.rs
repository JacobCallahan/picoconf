use pyo3::exceptions::{PyAttributeError, PyKeyError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::*;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::PathBuf;
use yaml_rust2::{Yaml, YamlLoader};

#[pyclass(name = "PicoConf", module = "picoconf._picoconf", mapping)]
pub struct PicoConf {
    inner: Py<PyDict>,
    cfg_path: Option<PathBuf>,
    name: Option<String>,
    envar_prefix: Option<String>,
    /// Maps lowercase key → original-cased key for O(1) case-insensitive lookup in pull_envars.
    key_case_map: HashMap<String, String>,
}

#[pymethods]
impl PicoConf {
    #[new]
    #[pyo3(signature = (cfg_path=None, **kwargs))]
    fn new(
        py: Python,
        cfg_path: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        // Create empty inner dict
        let inner = PyDict::new(py);

        let mut picoconf = PicoConf {
            inner: inner.clone().unbind(),
            cfg_path: None,
            name: None,
            envar_prefix: None,
            key_case_map: HashMap::new(),
        };

        // Process kwargs first
        if let Some(kwargs_dict) = kwargs {
            for (key, value) in kwargs_dict.iter() {
                picoconf.__setitem__(py, &key, value)?;
            }
        }

        // Handle cfg_path if provided
        if let Some(cfg_path_val) = cfg_path {
            picoconf.process_cfg_path(py, cfg_path_val)?;
        }

        // Apply env var overrides for all construction paths (file, kwargs, or both)
        picoconf.pull_envars(py)?;

        Ok(picoconf)
    }

    fn __len__(&self, py: Python) -> usize {
        self.inner.bind(py).len()
    }

    fn __bool__(&self, py: Python) -> bool {
        self.inner.bind(py).len() > 0
    }

    fn __contains__(&self, py: Python, key: &Bound<'_, PyAny>) -> PyResult<bool> {
        self.inner.bind(py).contains(key)
    }

    fn __iter__(&self, py: Python) -> PyResult<PyObject> {
        let dict = self.inner.bind(py);
        Ok(dict.call_method0("__iter__")?.unbind())
    }

    fn __getitem__(&self, py: Python, key: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let dict = self.inner.bind(py);
        match dict.get_item(key) {
            Ok(Some(value)) => Ok(value.unbind()),
            Ok(None) | Err(_) => Err(PyKeyError::new_err(format!("'{}'", key.str()?.to_str()?))),
        }
    }

    fn __setitem__(
        &mut self,
        py: Python,
        key: &Bound<'_, PyAny>,
        value: Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let key_str_opt = key.extract::<String>().ok();
        let is_envar_prefix = key_str_opt.as_deref() == Some("_envar_prefix");

        // Keep key_case_map in sync so pull_envars can resolve original casing
        if let Some(ref k) = key_str_opt {
            self.key_case_map.insert(k.to_lowercase(), k.clone());
        }

        let processed_value = self.wrap_dicts_in_value(py, value)?;

        // Store in inner dict
        self.inner.bind(py).set_item(key, &processed_value)?;

        // Special handling for _envar_prefix - dual storage
        if is_envar_prefix {
            // Normalize to lowercase for consistent cross-platform matching
            self.envar_prefix = Some(processed_value.extract::<String>()?.to_lowercase());
        }

        Ok(())
    }

    fn __delitem__(&mut self, py: Python, key: &Bound<'_, PyAny>) -> PyResult<()> {
        if let Ok(k) = key.extract::<String>() {
            self.key_case_map.remove(&k.to_lowercase());
        }
        self.inner.bind(py).del_item(key)
    }

    fn __eq__(&self, py: Python, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        self.inner.bind(py).eq(other)
    }

    #[pyo3(signature = (*args, **kwargs))]
    fn update(
        &mut self,
        py: Python,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        // Process positional argument (dict or iterable of pairs)
        if args.len() > 0 {
            let arg = args.get_item(0)?;

            // Try as dict first
            if let Ok(dict) = arg.downcast::<PyDict>() {
                for (key, value) in dict.iter() {
                    self.__setitem__(py, &key, value)?;
                }
            } else {
                // Try as iterable of pairs
                for item in arg.try_iter()? {
                    let pair = item?;
                    if let Ok(tuple) = pair.downcast::<PyTuple>() {
                        if tuple.len() == 2 {
                            let key = tuple.get_item(0)?;
                            let value = tuple.get_item(1)?;
                            self.__setitem__(py, &key, value)?;
                        }
                    }
                }
            }
        }

        // Process kwargs
        if let Some(kwargs_dict) = kwargs {
            for (key, value) in kwargs_dict.iter() {
                self.__setitem__(py, &key, value)?;
            }
        }

        Ok(())
    }

    fn keys(&self, py: Python) -> PyResult<PyObject> {
        Ok(self.inner.bind(py).call_method0("keys")?.unbind())
    }

    fn values(&self, py: Python) -> PyResult<PyObject> {
        Ok(self.inner.bind(py).call_method0("values")?.unbind())
    }

    fn items(&self, py: Python) -> PyResult<PyObject> {
        Ok(self.inner.bind(py).call_method0("items")?.unbind())
    }

    #[pyo3(signature = (key, default=None))]
    fn get(
        &self,
        py: Python,
        key: &Bound<'_, PyAny>,
        default: Option<PyObject>,
    ) -> PyResult<PyObject> {
        let dict = self.inner.bind(py);
        match dict.get_item(key) {
            Ok(Some(value)) => Ok(value.unbind()),
            Ok(None) | Err(_) => Ok(default.unwrap_or_else(|| py.None())),
        }
    }

    #[pyo3(signature = (key, default=None))]
    fn pop(
        &mut self,
        py: Python,
        key: &Bound<'_, PyAny>,
        default: Option<PyObject>,
    ) -> PyResult<PyObject> {
        let dict = self.inner.bind(py);
        match dict.get_item(key) {
            Ok(Some(value)) => {
                dict.del_item(key)?;
                Ok(value.unbind())
            }
            Ok(None) | Err(_) => Ok(default.unwrap_or_else(|| py.None())),
        }
    }

    fn popitem(&mut self, py: Python) -> PyResult<PyObject> {
        Ok(self.inner.bind(py).call_method0("popitem")?.unbind())
    }

    #[pyo3(signature = (key, default=None))]
    fn setdefault(
        &mut self,
        py: Python,
        key: &Bound<'_, PyAny>,
        default: Option<PyObject>,
    ) -> PyResult<PyObject> {
        let dict = self.inner.bind(py);
        match dict.get_item(key) {
            Ok(Some(value)) => Ok(value.unbind()),
            Ok(None) | Err(_) => {
                let default_val = default.unwrap_or_else(|| py.None());
                let default_bound = default_val.bind(py);
                self.__setitem__(py, key, default_bound.clone())?;
                Ok(default_val)
            }
        }
    }

    fn clear(&mut self, py: Python) -> PyResult<()> {
        self.inner.bind(py).clear();
        self.envar_prefix = None;
        self.key_case_map.clear();
        Ok(())
    }

    fn copy(&self, py: Python) -> PyResult<Self> {
        let new_inner = self.inner.bind(py).copy()?;
        Ok(PicoConf {
            inner: new_inner.unbind(),
            cfg_path: self.cfg_path.clone(),
            name: self.name.clone(),
            envar_prefix: self.envar_prefix.clone(),
            key_case_map: self.key_case_map.clone(),
        })
    }

    fn __getattr__(&self, py: Python, name: &str) -> PyResult<PyObject> {
        if name.starts_with('_') {
            // Handle private attributes
            match name {
                "_cfg_path" => {
                    if let Some(ref path) = self.cfg_path {
                        // Convert PathBuf to pathlib.Path
                        let pathlib = py.import("pathlib")?;
                        let path_cls = pathlib.getattr("Path")?;
                        let path_str = path
                            .to_str()
                            .ok_or_else(|| PyValueError::new_err("Invalid path encoding"))?;
                        Ok(path_cls.call1((path_str,))?.unbind())
                    } else {
                        Err(PyAttributeError::new_err(format!(
                            "'PicoConf' object has no attribute '{}'",
                            name
                        )))
                    }
                }
                "_name" => {
                    if let Some(ref name_val) = self.name {
                        Ok(PyString::new(py, name_val).into_any().unbind())
                    } else {
                        Err(PyAttributeError::new_err(format!(
                            "'PicoConf' object has no attribute '{}'",
                            name
                        )))
                    }
                }
                "_envar_prefix" => {
                    if let Some(ref prefix) = self.envar_prefix {
                        Ok(PyString::new(py, prefix).into_any().unbind())
                    } else {
                        Err(PyAttributeError::new_err(format!(
                            "'PicoConf' object has no attribute '{}'",
                            name
                        )))
                    }
                }
                _ => Err(PyAttributeError::new_err(format!(
                    "'PicoConf' object has no attribute '{}'",
                    name
                ))),
            }
        } else {
            // Look up in inner dict
            let dict = self.inner.bind(py);
            match dict.get_item(name) {
                Ok(Some(value)) => Ok(value.unbind()),
                Ok(None) | Err(_) => Err(PyAttributeError::new_err(format!(
                    "'PicoConf' object has no attribute '{}'",
                    name
                ))),
            }
        }
    }

    fn __setattr__(&mut self, py: Python, name: &str, value: Bound<'_, PyAny>) -> PyResult<()> {
        if name.starts_with('_') {
            // Handle private attributes
            match name {
                "_cfg_path" => {
                    let path_str = value.extract::<String>()?;
                    self.cfg_path = Some(PathBuf::from(path_str));
                }
                "_name" => {
                    self.name = Some(value.extract::<String>()?);
                }
                "_envar_prefix" => {
                    self.envar_prefix = Some(value.extract::<String>()?);
                }
                _ => {}
            }
            Ok(())
        } else {
            // Use __setitem__ for regular attributes
            let key = PyString::new(py, name);
            self.__setitem__(py, key.as_any(), value)
        }
    }

    fn to_dict(&self, py: Python) -> PyResult<PyObject> {
        self.to_dict_recursive(py, &self.inner.bind(py).clone().into_any())
    }

    fn __repr__(&self, py: Python) -> PyResult<String> {
        let name = self.name.as_deref().unwrap_or("dict");
        let dict = self.inner.bind(py);

        // Collect non-private keys
        let mut keys: Vec<String> = Vec::new();
        for key in dict.keys().iter() {
            if let Ok(key_str) = key.extract::<String>() {
                if !key_str.starts_with('_') {
                    keys.push(key_str);
                }
            }
        }

        let keys_str = keys.join(", ");
        Ok(format!("<PicoConf {}.({})>", name, keys_str))
    }

    fn __dir__(&self, py: Python) -> PyResult<Vec<PyObject>> {
        let mut result = Vec::new();

        // Add dict method names
        let methods = vec![
            "keys",
            "values",
            "items",
            "get",
            "pop",
            "popitem",
            "setdefault",
            "clear",
            "copy",
            "update",
            "to_dict",
        ];
        for method in methods {
            result.push(PyString::new(py, method).into_any().unbind());
        }

        // Add dunder methods
        let dunders = vec![
            "__getitem__",
            "__setitem__",
            "__delitem__",
            "__contains__",
            "__iter__",
            "__len__",
            "__bool__",
            "__eq__",
            "__repr__",
            "__getattr__",
            "__setattr__",
            "__dir__",
        ];
        for dunder in dunders {
            result.push(PyString::new(py, dunder).into_any().unbind());
        }

        // Add non-private keys from inner dict
        let dict = self.inner.bind(py);
        for key in dict.keys().iter() {
            if let Ok(key_str) = key.extract::<String>() {
                if !key_str.starts_with('_') {
                    result.push(key.unbind());
                }
            }
        }

        Ok(result)
    }
}

// Helper methods (not exposed to Python)
impl PicoConf {
    fn process_cfg_path(&mut self, py: Python, cfg_path: &Bound<'_, PyAny>) -> PyResult<()> {
        // Check if it's a string
        if let Ok(path_str) = cfg_path.extract::<String>() {
            self.handle_path(py, &path_str)?;
            return Ok(());
        }

        // Check if it's a pathlib.Path
        if let Ok(pathlib) = py.import("pathlib") {
            if let Ok(path_cls) = pathlib.getattr("Path") {
                if cfg_path.is_instance(&path_cls)? {
                    let path_str = cfg_path.str()?.to_str()?.to_string();
                    self.handle_path(py, &path_str)?;
                    return Ok(());
                }
            }
        }

        // Check if it's a dict
        if let Ok(dict) = cfg_path.downcast::<PyDict>() {
            self.update(py, &PyTuple::empty(py), Some(dict))?;
            return Ok(());
        }

        // For other types, do nothing (matches Python behavior)
        Ok(())
    }

    fn handle_path(&mut self, py: Python, path_str: &str) -> PyResult<()> {
        let path = PathBuf::from(path_str);

        if path.is_dir() {
            // Directory mode: load all .pconf files
            self.cfg_path = Some(path.clone());
            self.name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string());

            let entries = std::fs::read_dir(&path)
                .map_err(|e| PyValueError::new_err(format!("Cannot read directory: {}", e)))?;

            for entry in entries {
                let entry = entry
                    .map_err(|e| PyValueError::new_err(format!("Directory entry error: {}", e)))?;
                let entry_path = entry.path();

                if entry_path.extension().and_then(|s| s.to_str()) == Some("pconf") {
                    let stem = entry_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| PyValueError::new_err("Invalid filename"))?;

                    // Recursively create PicoConf for this file
                    let path_py = PyString::new(py, entry_path.to_str().unwrap());
                    let sub_conf = PicoConf::new(py, Some(path_py.as_any()), None)?;

                    // Store in inner dict
                    let key = PyString::new(py, stem);
                    let sub_conf_py = Py::new(py, sub_conf)?;
                    self.inner.bind(py).set_item(key, sub_conf_py)?;
                }
            }
        } else if path.extension().and_then(|s| s.to_str()) == Some("pconf") {
            // File mode: load single .pconf file
            self.cfg_path = Some(path.clone());
            self.name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string());

            self.load_config(py)?;
        }

        Ok(())
    }

    fn load_config(&mut self, py: Python) -> PyResult<()> {
        let path = self
            .cfg_path
            .as_ref()
            .ok_or_else(|| PyValueError::new_err("No config path set"))?;

        let content = std::fs::read_to_string(path)
            .map_err(|e| PyValueError::new_err(format!("Cannot read file: {}", e)))?;

        let docs = YamlLoader::load_from_str(&content)
            .map_err(|e| PyValueError::new_err(format!("YAML parse error: {}", e)))?;

        let doc = docs
            .into_iter()
            .next()
            .ok_or_else(|| PyValueError::new_err("Empty YAML document"))?;

        if let Yaml::Hash(mut hash) = doc {
            // Handle _import directive
            let import_key = Yaml::String("_import".to_string());
            if let Some(Yaml::Array(imports)) = hash.remove(&import_key) {
                let parent_dir = path
                    .parent()
                    .ok_or_else(|| PyValueError::new_err("Cannot get parent directory"))?;

                for import_path in imports {
                    if let Yaml::String(import_str) = import_path {
                        let full_path = parent_dir.join(&import_str);

                        if full_path.is_dir() {
                            // Import directory - create a PicoConf for the whole directory
                            let dir_stem = full_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .ok_or_else(|| PyValueError::new_err("Invalid directory name"))?;

                            let path_py = PyString::new(py, full_path.to_str().unwrap());
                            let sub_conf = PicoConf::new(py, Some(path_py.as_any()), None)?;

                            let key = PyString::new(py, dir_stem);
                            let sub_conf_py = Py::new(py, sub_conf)?;
                            self.inner.bind(py).set_item(key, sub_conf_py)?;
                        } else if full_path.extension().and_then(|s| s.to_str()) == Some("pconf") {
                            // Import single file
                            let stem = full_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .ok_or_else(|| PyValueError::new_err("Invalid filename"))?;

                            let path_py = PyString::new(py, full_path.to_str().unwrap());
                            let sub_conf = PicoConf::new(py, Some(path_py.as_any()), None)?;

                            let key = PyString::new(py, stem);
                            let sub_conf_py = Py::new(py, sub_conf)?;
                            self.inner.bind(py).set_item(key, sub_conf_py)?;
                        }
                    }
                }
            }

            // Convert remaining YAML to Python dict
            let py_dict = PyDict::new(py);
            for (key, value) in hash {
                let py_key = yaml_to_pyobject(py, &key)?;
                let py_value = yaml_to_pyobject(py, &value)?;
                py_dict.set_item(py_key, py_value)?;
            }

            // Update self with the dict (triggers PicoConf wrapping)
            self.update(py, &PyTuple::empty(py), Some(&py_dict))?;
        }

        Ok(())
    }

    fn pull_envars(&mut self, py: Python) -> PyResult<()> {
        if let Some(ref prefix) = self.envar_prefix {
            if !prefix.is_empty() {
                let full_prefix = format!("{}_", prefix);
                let full_prefix_lower = full_prefix.to_lowercase();

                // Collect matching env vars first to avoid holding the env iterator
                // while mutating self via __setitem__.
                let updates: Vec<(String, String)> = std::env::vars()
                    .filter_map(|(key, val)| {
                        let key_lower = key.to_lowercase();
                        if key_lower.starts_with(&full_prefix_lower) {
                            Some((key_lower[full_prefix_lower.len()..].to_string(), val))
                        } else {
                            None
                        }
                    })
                    .collect();

                for (config_key_lower, val) in updates {
                    // Resolve to original casing via the pure-Rust side table.
                    // Falls back to the lowercase suffix if no existing key matches.
                    let actual_key = self
                        .key_case_map
                        .get(&config_key_lower)
                        .cloned()
                        .unwrap_or_else(|| config_key_lower.clone());

                    let py_value = match serde_json::from_str::<JsonValue>(&val) {
                        Ok(json_val) => {
                            let py_obj = json_to_pyobject(py, &json_val)?;
                            let py_obj_bound = py_obj.bind(py);
                            let picoconf_val = PicoConf::new(py, Some(py_obj_bound), None)?;
                            Py::new(py, picoconf_val)?.into_any().into_bound(py)
                        }
                        Err(_) => PyString::new(py, &val).into_any(),
                    };

                    let key_py = PyString::new(py, &actual_key);
                    self.__setitem__(py, key_py.as_any(), py_value)?;
                }
            }
        }

        Ok(())
    }

    fn wrap_dicts_in_value<'py>(
        &self,
        py: Python<'py>,
        value: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // If value is a PyDict but NOT a PicoConf, wrap it
        if value.is_instance_of::<PyDict>() {
            // Check if it's already a PicoConf (will be false for plain dicts)
            if !value.is_instance_of::<PicoConf>() {
                let wrapped = PicoConf::new(py, Some(&value), None)?;
                return Ok(Py::new(py, wrapped)?.into_any().into_bound(py));
            }
        }

        // If value is a PyList, wrap any dict elements
        if let Ok(list) = value.downcast::<PyList>() {
            let new_list = PyList::empty(py);
            for item in list.iter() {
                let wrapped_item = self.wrap_dicts_in_value(py, item)?;
                new_list.append(wrapped_item)?;
            }
            return Ok(new_list.into_any());
        }

        Ok(value)
    }

    fn to_dict_recursive(&self, py: Python, obj: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        // If it's a PicoConf, call to_dict on it
        if obj.is_instance_of::<PicoConf>() {
            let nc = obj.extract::<PyRef<PicoConf>>()?;
            return nc.to_dict(py);
        }

        // If it's a list, recurse into elements
        if let Ok(list) = obj.downcast::<PyList>() {
            let new_list = PyList::empty(py);
            for item in list.iter() {
                let converted = self.to_dict_recursive(py, &item)?;
                new_list.append(converted)?;
            }
            return Ok(new_list.into_any().unbind());
        }

        // If it's a dict, recurse into values
        if let Ok(dict) = obj.downcast::<PyDict>() {
            let new_dict = PyDict::new(py);
            for (key, value) in dict.iter() {
                let converted = self.to_dict_recursive(py, &value)?;
                new_dict.set_item(key, converted)?;
            }
            return Ok(new_dict.into_any().unbind());
        }

        // Everything else, pass through
        Ok(obj.clone().unbind())
    }
}

// Convert YAML to Python objects
fn yaml_to_pyobject(py: Python, yaml: &Yaml) -> PyResult<PyObject> {
    match yaml {
        Yaml::String(s) => Ok(PyString::new(py, s).into_any().unbind()),
        Yaml::Integer(i) => Ok(i.into_pyobject(py)?.into_any().unbind()),
        Yaml::Real(s) => {
            let f: f64 = s
                .parse()
                .map_err(|_| PyValueError::new_err(format!("Invalid float: {}", s)))?;
            Ok(f.into_pyobject(py)?.into_any().unbind())
        }
        Yaml::Boolean(b) => {
            let bool_obj = b.into_pyobject(py)?;
            Ok(bool_obj.to_owned().into_any().unbind())
        }
        Yaml::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(yaml_to_pyobject(py, item)?)?;
            }
            Ok(list.into_any().unbind())
        }
        Yaml::Hash(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                let key = yaml_to_pyobject(py, k)?;
                let val = yaml_to_pyobject(py, v)?;
                dict.set_item(key, val)?;
            }
            Ok(dict.into_any().unbind())
        }
        Yaml::Null => Ok(py.None()),
        _ => Ok(py.None()), // BadValue, Alias
    }
}

// Convert JSON to Python objects
fn json_to_pyobject(py: Python, json: &JsonValue) -> PyResult<PyObject> {
    match json {
        JsonValue::Null => Ok(py.None()),
        JsonValue::Bool(b) => {
            let bool_obj = b.into_pyobject(py)?;
            Ok(bool_obj.to_owned().into_any().unbind())
        }
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_pyobject(py)?.into_any().unbind())
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_pyobject(py)?.into_any().unbind())
            } else {
                Ok(py.None())
            }
        }
        JsonValue::String(s) => Ok(PyString::new(py, s).into_any().unbind()),
        JsonValue::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(json_to_pyobject(py, item)?)?;
            }
            Ok(list.into_any().unbind())
        }
        JsonValue::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(k, json_to_pyobject(py, v)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}
