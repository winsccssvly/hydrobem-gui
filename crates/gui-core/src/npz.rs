//! NumPy's archive of arrays (`.npz`): a ZIP of `.npy` files (NumPy's format 1.0, C order),
//! stored uncompressed, which `numpy.load` reads without pickling. Written here without a
//! dependency: the export of the results (`files::export_npz`).

use std::borrow::Cow;
use std::io::{self, Write};

use hydrobem_engine::ElementType;

/// An array: its NumPy type (`descr`, e.g. `<f8`), its dimensions and its bytes in C order.
pub struct Array<'a> {
    pub descr: Cow<'static, str>,
    pub shape: Vec<usize>,
    pub data: Cow<'a, [u8]>,
}

/// The NumPy type of an engine element type (little-endian); None for a text.
pub fn descr(t: ElementType) -> Option<&'static str> {
    Some(match t {
        ElementType::Float64 => "<f8",
        ElementType::Float32 => "<f4",
        ElementType::Complex64 => "<c8",
        ElementType::Complex128 => "<c16",
        ElementType::Int32 => "<i4",
        ElementType::Int64 => "<i8",
        ElementType::Text => return None,
    })
}

impl Array<'static> {
    /// A text as a zero-dimensional array of NumPy's Unicode type (UTF-32, `<U`).
    pub fn text(text: &str) -> Array<'static> {
        let mut a = Array::texts(&[text]);
        a.shape.clear();
        a
    }

    /// Texts as a one-dimensional array of NumPy's Unicode type, as long as the longest.
    pub fn texts<S: AsRef<str>>(texts: &[S]) -> Array<'static> {
        let width = texts.iter().map(|t| t.as_ref().chars().count()).max().unwrap_or(0).max(1);
        let mut data = Vec::with_capacity(texts.len() * width * 4);
        for t in texts {
            let mut n = 0;
            for c in t.as_ref().chars() {
                data.extend_from_slice(&(c as u32).to_le_bytes());
                n += 1;
            }
            data.resize(data.len() + (width - n) * 4, 0);
        }
        Array { descr: format!("<U{width}").into(), shape: vec![texts.len()], data: data.into() }
    }
}

/// The `.npy` header of an array: the magic, version 1.0 and the dictionary, padded with spaces
/// so that the data start at a multiple of 64 bytes.
pub fn header(descr: &str, shape: &[usize]) -> Vec<u8> {
    let dims = match shape {
        [] => "()".to_string(),
        [n] => format!("({n},)"),
        _ => format!("({})", shape.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")),
    };
    let mut dict = format!("{{'descr': '{descr}', 'fortran_order': False, 'shape': {dims}, }}");
    let total = 10 + dict.len() + 1;
    dict.push_str(&" ".repeat((64 - total % 64) % 64));
    dict.push('\n');
    let mut h = Vec::with_capacity(10 + dict.len());
    h.extend_from_slice(b"\x93NUMPY\x01\x00");
    h.extend_from_slice(&(dict.len() as u16).to_le_bytes());
    h.extend_from_slice(dict.as_bytes());
    h
}

/// Writes the arrays as an `.npz` archive into `out`, each `NAME.npy`.
pub fn write<W: Write>(out: W, arrays: &[(String, Array)]) -> io::Result<W> {
    let mut zip = crate::zip::ZipWriter::new(out);
    for (name, a) in arrays {
        let count: usize = a.shape.iter().product();
        let size = a.descr[2..].parse::<usize>().unwrap_or(0) * if a.descr.starts_with("<U") { 4 } else { 1 };
        if count * size != a.data.len() {
            return Err(io::Error::other(format!("{name}: {} bytes for the shape {:?}", a.data.len(), a.shape)));
        }
        zip.add(&format!("{name}.npy"), &[&header(&a.descr, &a.shape), &a.data])?;
    }
    zip.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_align_the_data() {
        for shape in [&[][..], &[5], &[5, 6, 6], &[123_456, 1, 6]] {
            let h = header("<c8", shape);
            assert_eq!(h.len() % 64, 0);
            assert_eq!(*h.last().unwrap(), b'\n');
        }
        let h = header("<f8", &[5]);
        assert!(String::from_utf8_lossy(&h).contains("'shape': (5,), }"));
    }

    #[test]
    fn texts_are_utf32_of_the_longest() {
        let a = Array::texts(&["ab", "é"]);
        assert_eq!(a.descr, "<U2");
        assert_eq!(a.data.len(), 16);
        assert_eq!(&a.data[8..12], &0xe9u32.to_le_bytes());
        assert!(Array::text("x").shape.is_empty());
    }

    #[test]
    fn a_wrong_size_is_refused() {
        let a = Array { descr: "<f8".into(), shape: vec![2], data: vec![0u8; 8].into() };
        assert!(write(Vec::new(), &[("x".into(), a)]).is_err());
    }
}
