//! NISTCOM comments: `NAME value` lines that NIST software writes into a
//! comment segment to describe the image.

/// Prefix that identifies a NISTCOM comment.
const HEADER: &[u8] = b"NIST_COM";

/// Splits NISTCOM text into its fields the way the reference
/// implementation does: a name runs to the first space or tab, and its
/// value from the next non-blank byte to the end of the line.
fn parse_fields(text: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let is_blank = |b: u8| b == b' ' || b == b'\t';
    let mut fields = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let start = i;
        while i < text.len() && !is_blank(text[i]) {
            i += 1;
        }
        let name = text[start..i].to_vec();
        while i < text.len() && is_blank(text[i]) {
            i += 1;
        }
        let start = i;
        while i < text.len() && text[i] != b'\n' {
            i += 1;
        }
        let value = text[start..i].to_vec();
        while i < text.len() && (is_blank(text[i]) || text[i] == b'\n') {
            i += 1;
        }
        fields.push((name, value));
    }
    fields
}

/// Sets field `name`, in place if it exists, at the end otherwise.
fn set(fields: &mut Vec<(Vec<u8>, Vec<u8>)>, name: &str, value: String) {
    let value = value.into_bytes();
    match fields.iter_mut().find(|(n, _)| n == name.as_bytes()) {
        Some(field) => field.1 = value,
        None => fields.push((name.as_bytes().to_vec(), value)),
    }
}

/// Builds the NISTCOM comment the reference encoder writes, merged into
/// the caller's NISTCOM comment `base` if there is one.
pub(crate) fn build(
    width: usize,
    height: usize,
    ppi: Option<u32>,
    bitrate: f32,
    base: Option<&[u8]>,
) -> Vec<u8> {
    let mut fields = match base {
        Some(base) => parse_fields(text(base)),
        None => vec![(HEADER.to_vec(), b"6".to_vec())],
    };
    let ppi = ppi.map_or_else(|| "-1".to_owned(), |p| p.to_string());
    set(&mut fields, "PIX_WIDTH", width.to_string());
    set(&mut fields, "PIX_HEIGHT", height.to_string());
    set(&mut fields, "PIX_DEPTH", "8".to_owned());
    set(&mut fields, "PPI", ppi);
    set(&mut fields, "LOSSY", "1".to_owned());
    set(&mut fields, "COLORSPACE", "GRAY".to_owned());
    set(&mut fields, "COMPRESSION", "WSQ".to_owned());
    set(
        &mut fields,
        "WSQ_BITRATE",
        format!("{:.6}", f64::from(bitrate)),
    );
    let count = fields.len().to_string();
    set(&mut fields, "NIST_COM", count);

    let mut out = Vec::new();
    for (i, (name, value)) in fields.iter().enumerate() {
        if i > 0 {
            out.push(b'\n');
        }
        out.extend_from_slice(name);
        if !value.is_empty() {
            out.push(b' ');
            out.extend_from_slice(value);
        }
    }
    out
}

/// The text of a comment, which may carry a NUL terminator.
fn text(comment: &[u8]) -> &[u8] {
    comment.split(|&b| b == 0).next().unwrap_or_default()
}

/// Whether `comment` is a NISTCOM comment.
pub(crate) fn is_nistcom(comment: &[u8]) -> bool {
    text(comment).starts_with(HEADER)
}

/// Returns the value of field `name` if `comment` is a NISTCOM comment
/// that has it.
pub(crate) fn field<'a>(comment: &'a [u8], name: &str) -> Option<&'a [u8]> {
    if !is_nistcom(comment) {
        return None;
    }
    let text = text(comment);
    text.split(|&b| b == b'\n').find_map(|line| {
        let is_space = |b: &u8| *b == b' ' || *b == b'\t';
        let start = line.iter().position(|b| !is_space(b))?;
        let line = &line[start..];
        let end = line.iter().position(is_space).unwrap_or(line.len());
        let (key, rest) = line.split_at(end);
        let value_start = rest.iter().position(|b| !is_space(b)).unwrap_or(rest.len());
        (key == name.as_bytes()).then_some(rest[value_start..].trim_ascii_end())
    })
}

/// The resolution recorded in a NISTCOM comment; unknown (-1) or invalid
/// values give `None`.
pub(crate) fn ppi(comment: &[u8]) -> Option<u32> {
    let value = field(comment, "PPI")?;
    std::str::from_utf8(value)
        .ok()?
        .parse()
        .ok()
        .filter(|&p| p > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_str(base: Option<&str>, ppi: Option<u32>) -> String {
        String::from_utf8(build(545, 622, ppi, 0.75, base.map(str::as_bytes))).unwrap()
    }

    #[test]
    fn reference_comment() {
        assert_eq!(
            build_str(None, Some(500)),
            "NIST_COM 9\nPIX_WIDTH 545\nPIX_HEIGHT 622\nPIX_DEPTH 8\nPPI 500\nLOSSY 1\n\
             COLORSPACE GRAY\nCOMPRESSION WSQ\nWSQ_BITRATE 0.750000"
        );
        assert!(build_str(None, None).contains("\nPPI -1\n"));
    }

    #[test]
    fn merges_a_caller_comment() {
        let base = "NIST_COM 4\nSEX m\nPPI 1000\nFING_CLASS W \n\n";
        assert_eq!(
            build_str(Some(base), Some(500)),
            "NIST_COM 11\nSEX m\nPPI 500\nFING_CLASS W \nPIX_WIDTH 545\nPIX_HEIGHT 622\n\
             PIX_DEPTH 8\nLOSSY 1\nCOLORSPACE GRAY\nCOMPRESSION WSQ\nWSQ_BITRATE 0.750000"
        );
    }

    #[test]
    fn reads_ppi() {
        assert_eq!(ppi(build_str(None, Some(1000)).as_bytes()), Some(1000));
        assert_eq!(ppi(build_str(None, None).as_bytes()), None);
        assert_eq!(ppi(b"NIST_COM 2\nPPI\t 500 \n\0junk"), Some(500));
        assert_eq!(ppi(b"PPI 500"), None);
    }
}
