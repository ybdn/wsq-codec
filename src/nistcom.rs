//! NISTCOM comments: `NAME value` lines that NIST software writes into a
//! comment segment to describe the image.

/// Prefix that identifies a NISTCOM comment.
const HEADER: &[u8] = b"NIST_COM";

/// Builds the NISTCOM comment the reference encoder writes.
pub(crate) fn build(width: usize, height: usize, ppi: Option<u32>, bitrate: f32) -> String {
    let ppi = ppi.map_or_else(|| "-1".to_owned(), |p| p.to_string());
    let fields = [
        ("PIX_WIDTH", width.to_string()),
        ("PIX_HEIGHT", height.to_string()),
        ("PIX_DEPTH", "8".to_owned()),
        ("PPI", ppi),
        ("LOSSY", "1".to_owned()),
        ("COLORSPACE", "GRAY".to_owned()),
        ("COMPRESSION", "WSQ".to_owned()),
        ("WSQ_BITRATE", format!("{:.6}", f64::from(bitrate))),
    ];
    let mut s = format!("NIST_COM {}", fields.len() + 1);
    for (name, value) in fields {
        s.push('\n');
        s.push_str(name);
        s.push(' ');
        s.push_str(&value);
    }
    s
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

    #[test]
    fn reference_comment() {
        assert_eq!(
            build(545, 622, Some(500), 0.75),
            "NIST_COM 9\nPIX_WIDTH 545\nPIX_HEIGHT 622\nPIX_DEPTH 8\nPPI 500\nLOSSY 1\n\
             COLORSPACE GRAY\nCOMPRESSION WSQ\nWSQ_BITRATE 0.750000"
        );
        assert!(build(1, 1, None, 2.25).contains("\nPPI -1\n"));
    }

    #[test]
    fn reads_ppi() {
        let c = build(545, 622, Some(1000), 0.75);
        assert_eq!(ppi(c.as_bytes()), Some(1000));
        assert_eq!(ppi(build(1, 1, None, 0.75).as_bytes()), None);
        assert_eq!(ppi(b"NIST_COM 2\nPPI\t 500 \n\0junk"), Some(500));
        assert_eq!(ppi(b"PPI 500"), None);
    }
}
