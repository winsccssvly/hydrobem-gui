//! The errors of the engine and the place they name; the diagnostics (warnings) of a model.

use std::fmt;

use crate::codes::{Severity, Warning};
use crate::ffi::HbLocation;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The place an error or a diagnostic names: zero-based indices, the mesh element one-based.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Location {
    pub body: Option<usize>,
    pub mesh: Option<usize>,
    pub element: Option<usize>,
    pub field_point: Option<usize>,
    pub lid: Option<usize>,
    pub frequency: Option<usize>,
}

impl Location {
    pub(crate) fn from_c(l: &HbLocation) -> Location {
        let at = |i: i64| usize::try_from(i).ok();
        Location {
            body: at(l.body),
            mesh: at(l.mesh),
            element: at(l.element),
            field_point: at(l.field_point),
            lid: at(l.lid),
            frequency: at(l.frequency),
        }
    }
}

/// The kind of an error (the `HB_ERR_` code of the C ABI).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Value,
    Type,
    Unsupported,
    State,
    Cancelled,
    Internal,
    Argument,
    Numerical,
    Io,
    Exists,
}

/// An error of the engine: its kind, message and place.
#[derive(Clone, Debug)]
pub struct Error(Box<(ErrorKind, String, Location)>);

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Error {
        Error(Box::new((kind, message.into(), Location::default())))
    }

    pub fn value(message: impl Into<String>) -> Error {
        Error::new(ErrorKind::Value, message)
    }

    pub fn at(mut self, location: Location) -> Error {
        self.0.2 = location;
        self
    }

    pub fn kind(&self) -> ErrorKind {
        self.0.0
    }

    pub fn message(&self) -> &str {
        &self.0.1
    }

    pub fn location(&self) -> &Location {
        &self.0.2
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for Error {}

/// A warning or an error of the model rules or of a solve.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: i32,
    pub location: Location,
    pub message: String,
}

impl Diagnostic {
    /// The warning of its code (None for an error).
    pub fn warning(&self) -> Option<Warning> {
        if self.severity == Severity::Error { None } else { Warning::from_code(self.code) }
    }

    /// The rows of a diagnostics table.
    pub(crate) fn from_table(t: &crate::table::Table) -> Result<Vec<Diagnostic>> {
        let severity = t.i32s("severity")?;
        let code = t.i32s("code")?;
        let message = t.texts("message")?;
        let column = |name| t.i64s(name);
        let (body, mesh, element) = (column("body")?, column("mesh")?, column("element")?);
        let (field_point, lid, frequency) = (column("field_point")?, column("lid")?, column("frequency")?);
        let n = message.len();
        let lengths = [severity.len(), code.len(), body.len(), mesh.len(), element.len(), field_point.len()];
        if lengths.iter().chain(&[lid.len(), frequency.len()]).any(|&l| l != n) {
            return Err(Error::new(ErrorKind::Internal, "a diagnostics table with columns of different lengths"));
        }
        Ok((0..n)
            .map(|k| Diagnostic {
                severity: if severity[k] != 0 { Severity::Error } else { Severity::Warning },
                code: code[k],
                location: Location::from_c(&HbLocation {
                    body: body[k],
                    mesh: mesh[k],
                    element: element[k],
                    field_point: field_point[k],
                    lid: lid[k],
                    frequency: frequency[k],
                }),
                message: message[k].clone(),
            })
            .collect())
    }
}
