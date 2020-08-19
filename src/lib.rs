mod text;

use pyo3::exceptions::{Exception, NotImplementedError, ValueError};
use pyo3::proc_macro::{pyclass, pymethods, pymodule};
use pyo3::type_object::PyTypeObject;
use pyo3::types::{IntoPyDict, PyAny, PyDict, PyModule};
use pyo3::{create_exception, wrap_pymodule, PyObject, PyResult, Python};
use serde_python_typing::{DualError, Type};
use std::fmt;
use text::{split_and_dedent, IterLines, JoinLines, LineIter};

fn str_to_char(s: &str) -> PyResult<char> {
  let mut chars = s.chars();
  match (chars.next(), chars.next()) {
    (Some(ch), None) => Ok(ch),
    _ => Err(ValueError::py_err(format!("expected a single character but got '{}'", s))),
  }
}

create_exception!(stringly, StringlyError, Exception);
create_exception!(stringly, SerializationError, StringlyError);
create_exception!(stringly, ImportFunctionError, StringlyError);

#[pymodule]
fn error(py: Python, m: &PyModule) -> PyResult<()> {
  m.setattr("StringlyError", StringlyError::type_object(py))?;
  m.setattr("SerializationError", SerializationError::type_object(py))?;
  m.setattr("ImportFunctionError", ImportFunctionError::type_object(py))?;

  Ok(())
}

#[pyclass]
struct DocString {
  doc: String,
  #[pyo3(get)]
  text: String,
  defaults: Vec<(String, String)>,
  argdocs: Vec<(String, String)>,
  presets: Vec<(String, Vec<(String, String)>)>,
}

#[pymethods]
impl DocString {
  #[new]
  fn new(f: &PyAny) -> PyResult<Self> {
    let doc = if let Ok(doc) = f.getattr("__doc__") { doc.extract()? } else { "" };
    let lines = split_and_dedent(doc);

    let doc = lines.iter().copied().join_lines();

    let mut text = String::new();
    let mut defaults = Vec::new();
    let mut argdocs = Vec::new();
    let mut presets = Vec::new();
    let mut lines = lines.iter_lines();
    lines.gobble_empty_lines();
    // Parse blocks separated by white lines.
    while let Some(line) = lines.peek_unempty() {
      // Gobble empty lines.
      lines.gobble_empty_lines();
      if line == ".. arguments::" {
        lines.next();
        let mut lines = lines.dedent(3);
        while let Some(line) = lines.next() {
          let arg = line.trim();
          let arg = match (arg.find(" ["), arg.ends_with(']')) {
            (Some(index), true) => {
              defaults.push((arg[..index].to_string(), arg[index + 2..arg.len() - 1].to_string()));
              &arg[..index]
            }
            _ => arg,
          };
          let description = lines.dedent(2).join_lines();
          argdocs.push((arg.to_string(), description.trim_end().to_string()));
        }
      } else if line == ".. presets::" {
        lines.next();
        let mut lines = lines.dedent(3);
        while let Some(line) = lines.next() {
          let preset = line.trim();
          let mut parameters = Vec::new();
          use stringly::util::{deprettify, safesplit, safesplit_once, unprotect};
          // FIXME: handle deprettify error
          let parameter_string = deprettify(&lines.dedent(2).join_lines()).unwrap();
          for si in safesplit(&parameter_string, ',') {
            if let Ok((key, value)) = safesplit_once(si, '=') {
              parameters.push((unprotect(key).to_string(), unprotect(value).to_string()));
            } else {
              return Err(SerializationError::py_err(format!("preset {} has no value for argument {}", preset, unprotect(si))));
            }
          }
          presets.push((preset.to_string(), parameters));
        }
      } else {
        // Insert a white line unless this is the first paragraph.
        if !text.is_empty() {
          text.push('\n');
        }
        // Copy a single paragraph.
        while let Some(line) = lines.next_if_unempty() {
          text.push_str(line);
          text.push('\n');
        }
      }
    }

    Ok(DocString { doc, text: text.trim().to_string(), defaults, argdocs, presets })
  }
  fn __str__(&self) -> PyResult<&str> {
    Ok(&self.doc)
  }
  #[getter]
  fn defaults<'py>(&self, py: Python<'py>) -> &'py PyDict {
    self.defaults[..].into_py_dict(py)
  }
  #[getter]
  fn argdocs<'py>(&self, py: Python<'py>) -> &'py PyDict {
    self.argdocs[..].into_py_dict(py)
  }
  #[getter]
  fn presets<'py>(&self, py: Python<'py>) -> &'py PyDict {
    let p: Vec<(&str, &PyDict)> = self.presets.iter().map(|(key, value)| (key.as_str(), value.into_py_dict(py))).collect();
    p[..].into_py_dict(py)
  }
}

#[pymodule]
fn util(_py: Python, m: &PyModule) -> PyResult<()> {
  #[pyfn(m, "safesplit")]
  #[text_signature = "(s, sep, /)"]
  fn safesplit<'a>(s: &'a str, sep: &str) -> PyResult<Vec<&'a str>> {
    let mut chars = sep.chars();
    match (chars.next(), chars.next()) {
      (Some(ch), None) => Ok(stringly::util::safesplit(s, ch).collect()),
      _ => Err(ValueError::py_err("expected a separator of length 1")),
    }
  }

  #[pyfn(m, "safesplit_once")]
  #[text_signature = "(s, sep, /)"]
  fn safesplit_once<'a>(s: &'a str, sep: &str) -> PyResult<(&'a str, &'a str)> {
    let mut chars = sep.chars();
    match (chars.next(), chars.next()) {
      (Some(ch), None) => match stringly::util::safesplit_once(s, ch) {
        Ok((l, r)) => Ok((l, r)),
        Err(e) => Err(ValueError::py_err(format!("{:?}", e))),
      },
      _ => Err(ValueError::py_err("expected a separator of length 1")),
    }
  }

  #[pyfn(m, "protect_unconditionally")]
  #[text_signature = "(s, /)"]
  fn protect_unconditionally(s: &str) -> String {
    stringly::util::protect_unconditionally(s)
  }

  #[pyfn(m, "protect_unbalanced")]
  #[text_signature = "(s, /)"]
  fn protect_unbalanced(s: &str) -> String {
    stringly::util::protect_unbalanced(s)
  }

  #[pyfn(m, "protect_regex")]
  #[text_signature = "(s, sep, /)"]
  fn protect_regex<'a>(s: &str, sep: &str) -> PyResult<String> {
    let items: Vec<&str> = sep.split('|').collect();
    match items.len() {
      1 => Ok(stringly::util::protect(s, str_to_char(items[0])?)),
      2 => Ok(stringly::util::protect(s, [str_to_char(items[0])?, str_to_char(items[1])?])),
      _ => Err(NotImplementedError::py_err(format!("only one or two characters are supported but got {}", items.len()))),
    }
  }

  #[pyfn(m, "unprotect")]
  #[text_signature = "(s, /)"]
  fn unprotect(s: &str) -> &str {
    stringly::util::unprotect(s)
  }

  #[pyfn(m, "is_balanced")]
  #[text_signature = "(s, /)"]
  fn is_balanced(s: &str) -> bool {
    stringly::util::is_balanced(s)
  }

  #[pyfn(m, "prettify")]
  #[text_signature = "(s, /)"]
  fn prettify(s: &str) -> String {
    stringly::util::prettify(s)
  }

  #[pyfn(m, "deprettify")]
  #[text_signature = "(s, /)"]
  fn deprettify(s: &str) -> PyResult<String> {
    match stringly::util::deprettify(s) {
      Ok(v) => Ok(v),
      Err(e) => Err(ValueError::py_err(format!("{:?}", e))),
    }
  }

  m.add_class::<DocString>()?;

  Ok(())
}

#[pymodule]
/// Stringly
/// ========
///
/// Human readable object serialization.
fn stringly(_py: Python, m: &PyModule) -> PyResult<()> {
  m.add_wrapped(wrap_pymodule!(error))?;
  m.add_wrapped(wrap_pymodule!(util))?;

  #[pyfn(m, "get_type_str")]
  #[text_signature = "(type, /)"]
  fn get_type_str(_py: Python, ty: &PyAny) -> PyResult<String> {
    Ok(Type::from_python(ty)?.to_string())
  }

  #[pyfn(m, "dumps")]
  #[text_signature = "(type, value, /)"]
  fn dumps(_py: Python, ty: &PyAny, val: &PyAny) -> PyResult<String> {
    let ty = &Type::from_python(ty)?;
    wrap_err(ty.serialize(stringly::Serializer, val))
  }

  #[pyfn(m, "loads")]
  #[text_signature = "(type, value, /)"]
  fn loads(py: Python, ty: &PyAny, val: &str) -> PyResult<PyObject> {
    let de = stringly::Deserializer::from_str(val);
    let ty = &Type::from_python(ty)?;
    wrap_err(ty.deserialize(de, py))
  }

  Ok(())
}

fn wrap_err<T, E: fmt::Display>(r: Result<T, DualError<E>>) -> PyResult<T> {
  match r {
    Ok(v) => Ok(v),
    Err(DualError::Python(e)) => Err(e),
    Err(DualError::Serialization(e)) => Err(SerializationError::py_err(format!("{}", e))),
  }
}
