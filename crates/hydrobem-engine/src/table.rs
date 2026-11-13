//! The tables of the DLL (`HbTable`: named columns) and the status of its calls.
#![allow(unsafe_code)] // the elements of the DLL's columns, the texts it hands out

use std::ffi::{CStr, CString, c_char};

use crate::error::{Error, ErrorKind, Location, Result};
use crate::ffi::{self, Api, HbColumn};

/// The functions of the DLL, or the error of its loading.
pub(crate) fn api() -> Result<&'static Api> {
    ffi::api().map_err(|m| Error::new(ErrorKind::Internal, m))
}

fn kind(status: ffi::Status) -> ErrorKind {
    match status {
        1 => ErrorKind::Value,
        2 => ErrorKind::Type,
        3 => ErrorKind::Unsupported,
        4 => ErrorKind::State,
        5 => ErrorKind::Cancelled,
        7 => ErrorKind::Argument,
        8 => ErrorKind::Numerical,
        9 => ErrorKind::Io,
        10 => ErrorKind::Exists,
        _ => ErrorKind::Internal,
    }
}

/// `Ok` for `HB_OK`, else the error of the last failed call of this thread.
pub(crate) fn check(a: &Api, status: ffi::Status) -> Result<()> {
    if status == 0 {
        return Ok(());
    }
    // SAFETY: the library's own text, valid until the next failure on this thread
    let message = unsafe { text_of((a.hb_last_error_message)()) };
    let mut l = ffi::HbLocation { body: -1, mesh: -1, element: -1, field_point: -1, lid: -1, frequency: -1 };
    // SAFETY: a writable location
    unsafe { (a.hb_last_error_location)(&mut l) };
    Err(Error::new(kind(status), message).at(Location::from_c(&l)))
}

/// A NUL-terminated UTF-8 text of the library (empty for null).
///
/// # Safety
/// `p` is null or a NUL-terminated string that stays valid during the call.
pub(crate) unsafe fn text_of(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: the caller's contract
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// A text handed out by the library, freed.
///
/// # Safety
/// `p` is null or a text of the library not yet freed.
pub(crate) unsafe fn owned_text(a: &Api, p: *mut c_char) -> String {
    // SAFETY: the caller's contract
    let s = unsafe { text_of(p) };
    // SAFETY: freed once, by the library that made it
    unsafe { (a.hb_string_free)(p) };
    s
}

/// A text for the library.
pub(crate) fn c(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| Error::new(ErrorKind::Argument, format!("{s:?}: a text with a NUL character")))
}

/// A path for the library.
pub(crate) fn c_path(p: &std::path::Path) -> Result<CString> {
    let s = p
        .to_str()
        .ok_or_else(|| Error::new(ErrorKind::Argument, format!("{}: the path is not valid UTF-8", p.display())))?;
    c(s)
}

/// A table of the library, freed when dropped.
pub(crate) struct Table {
    a: &'static Api,
    p: *mut ffi::Table,
}

impl Drop for Table {
    fn drop(&mut self) {
        // SAFETY: a table of the library, freed once
        unsafe { (self.a.hb_table_free)(self.p) };
    }
}

/// Calls a function that hands out a table.
pub(crate) fn table(f: impl FnOnce(&'static Api, *mut *mut ffi::Table) -> ffi::Status) -> Result<Table> {
    let a = api()?;
    let mut p = std::ptr::null_mut();
    let status = f(a, &mut p);
    let t = Table { a, p };
    check(a, status)?;
    if p.is_null() {
        return Err(Error::new(ErrorKind::Internal, "hydrobem.dll handed out no table"));
    }
    Ok(t)
}

/// A Rust type of the elements of a column, with its element code (`HB_F64`, ...): a column is read
/// only as the type of its code.
pub(crate) trait Element: sealed::Sealed + Copy {
    const DTYPE: i32;
}

mod sealed {
    pub trait Sealed {}
}

/// A text element: a NUL-terminated UTF-8 string of the library.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub(crate) struct Text(*const c_char);

macro_rules! element {
    ($($t:ty => $code:expr),*) => {$(
        impl sealed::Sealed for $t {}
        impl Element for $t {
            const DTYPE: i32 = $code;
        }
    )*};
}

element!(f64 => ffi::HB_F64, f32 => ffi::HB_F32, i32 => ffi::HB_I32, i64 => ffi::HB_I64, u64 => ffi::HB_U64,
    num_complex::Complex32 => ffi::HB_C64, num_complex::Complex64 => ffi::HB_C128, Text => ffi::HB_TEXT);

/// A column of a table or the view of a quantity, valid while its owner lives.
#[derive(Clone, Copy)]
pub(crate) struct Column<'a> {
    c: HbColumn,
    _owner: std::marker::PhantomData<&'a ()>,
}

impl<'a> Column<'a> {
    /// # Safety
    /// The pointers of `c` stay valid for `'a`.
    pub(crate) unsafe fn new(c: HbColumn) -> Column<'a> {
        Column { c, _owner: std::marker::PhantomData }
    }

    pub(crate) fn dtype(&self) -> i32 {
        self.c.dtype
    }

    pub(crate) fn shape(&self) -> &'a [usize] {
        if self.c.ndim == 0 || self.c.shape.is_null() {
            return &[];
        }
        // SAFETY: ndim sizes, valid for 'a
        unsafe { std::slice::from_raw_parts(self.c.shape, self.c.ndim) }
    }

    fn name(&self) -> String {
        // SAFETY: the column's name, valid for 'a
        unsafe { text_of(self.c.name) }
    }

    /// The elements of a column whose element type is `T`'s (`Element::DTYPE`).
    pub(crate) fn elements<T: Element>(&self) -> Result<&'a [T]> {
        if self.c.dtype != T::DTYPE {
            return Err(Error::new(
                ErrorKind::Type,
                format!("{}: elements of type {}, not {}", self.name(), self.c.dtype, T::DTYPE),
            ));
        }
        if self.c.count == 0 || self.c.data.is_null() {
            return Ok(&[]);
        }
        // SAFETY: count elements of this type (the dtype checked), aligned, valid for 'a
        Ok(unsafe { std::slice::from_raw_parts(self.c.data.cast::<T>(), self.c.count) })
    }

    /// The elements of a numeric column as bytes.
    pub(crate) fn bytes(&self) -> Result<&'a [u8]> {
        let size = match self.c.dtype {
            ffi::HB_F32 | ffi::HB_I32 => 4,
            ffi::HB_F64 | ffi::HB_I64 | ffi::HB_U64 | ffi::HB_C64 => 8,
            ffi::HB_C128 => 16,
            _ => {
                return Err(Error::new(
                    ErrorKind::Type,
                    format!("{}: elements of type {}, not numbers", self.name(), self.c.dtype),
                ));
            }
        };
        if self.c.count == 0 || self.c.data.is_null() {
            return Ok(&[]);
        }
        // SAFETY: count numeric elements of `size` bytes each (the dtype checked), valid for 'a
        Ok(unsafe { std::slice::from_raw_parts(self.c.data.cast::<u8>(), self.c.count * size) })
    }

    pub(crate) fn texts(&self) -> Result<Vec<String>> {
        let p = self.elements::<Text>()?;
        // SAFETY: the strings of a text column, valid for 'a
        Ok(p.iter().map(|s| unsafe { text_of(s.0) }).collect())
    }
}

impl Table {
    pub(crate) fn column(&self, name: &str) -> Result<Column<'_>> {
        let n = c(name)?;
        let mut col = HbColumn::default();
        // SAFETY: a table of the library, a NUL-terminated name, a writable column
        check(self.a, unsafe { (self.a.hb_table_find)(self.p, n.as_ptr(), &mut col) })?;
        // SAFETY: the column's pointers live as long as the table
        Ok(unsafe { Column::new(col) })
    }

    pub(crate) fn f64s(&self, name: &str) -> Result<Vec<f64>> {
        Ok(self.column(name)?.elements::<f64>()?.to_vec())
    }

    pub(crate) fn i32s(&self, name: &str) -> Result<Vec<i32>> {
        Ok(self.column(name)?.elements::<i32>()?.to_vec())
    }

    pub(crate) fn i64s(&self, name: &str) -> Result<Vec<i64>> {
        Ok(self.column(name)?.elements::<i64>()?.to_vec())
    }

    pub(crate) fn u64s(&self, name: &str) -> Result<Vec<u64>> {
        Ok(self.column(name)?.elements::<u64>()?.to_vec())
    }

    pub(crate) fn texts(&self, name: &str) -> Result<Vec<String>> {
        self.column(name)?.texts()
    }

    pub(crate) fn shape(&self, name: &str) -> Result<Vec<usize>> {
        Ok(self.column(name)?.shape().to_vec())
    }

    fn first<T: Copy>(v: &[T], name: &str) -> Result<T> {
        v.first().copied().ok_or_else(|| Error::new(ErrorKind::Internal, format!("{name}: an empty column")))
    }

    pub(crate) fn f64(&self, name: &str) -> Result<f64> {
        Table::first(&self.f64s(name)?, name)
    }

    pub(crate) fn i64(&self, name: &str) -> Result<i64> {
        Table::first(&self.i64s(name)?, name)
    }

    pub(crate) fn i32(&self, name: &str) -> Result<i32> {
        Table::first(&self.i32s(name)?, name)
    }

    pub(crate) fn u64(&self, name: &str) -> Result<u64> {
        Table::first(&self.u64s(name)?, name)
    }

    /// A 0-or-1 column.
    pub(crate) fn bool(&self, name: &str) -> Result<bool> {
        Ok(self.i32(name)? != 0)
    }

    pub(crate) fn text(&self, name: &str) -> Result<String> {
        self.texts(name)?.into_iter().next().ok_or_else(|| Error::new(ErrorKind::Internal, format!("{name}: no text")))
    }

    /// A count column (non-negative).
    pub(crate) fn count(&self, name: &str) -> Result<usize> {
        Ok(usize::try_from(self.i64(name)?).unwrap_or(0))
    }

    /// Rows of N reals.
    pub(crate) fn rows<const N: usize>(&self, name: &str) -> Result<Vec<[f64; N]>> {
        Ok(self.f64s(name)?.as_chunks::<N>().0.to_vec())
    }

    /// Rows of N integers.
    pub(crate) fn int_rows<const N: usize>(&self, name: &str) -> Result<Vec<[i32; N]>> {
        Ok(self.i32s(name)?.as_chunks::<N>().0.to_vec())
    }

    /// Rows of 4 points.
    pub(crate) fn quads(&self, name: &str) -> Result<Vec<[[f64; 3]; 4]>> {
        Ok(self.rows::<3>(name)?.as_chunks::<4>().0.to_vec())
    }
}
