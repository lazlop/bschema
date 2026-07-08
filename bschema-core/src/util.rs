//! Small string helpers ported from `graph-pattern-id/bschema/utils.py`.

/// Local name of an IRI: the part after the last `#`, or after the last `/`
/// if there is no `#`.
pub fn local_name(iri: &str) -> &str {
    let iri = iri.trim_start_matches('<').trim_end_matches('>');
    if let Some(idx) = iri.rfind('#') {
        &iri[idx + 1..]
    } else if let Some(idx) = iri.rfind('/') {
        &iri[idx + 1..]
    } else {
        iri
    }
}

/// Returns the common-prefix + common-suffix "pattern" shared by all
/// `strings`, ported from `common_pattern` in utils.py (used to derive a
/// class name from a group of equivalent subject IRIs).
pub fn common_pattern<S: AsRef<str>>(strings: &[S]) -> String {
    let strs: Vec<&str> = strings.iter().map(|s| s.as_ref()).collect();
    if strs.is_empty() {
        return String::new();
    }

    let chars: Vec<Vec<char>> = strs.iter().map(|s| s.chars().collect()).collect();
    let min_len = chars.iter().map(|c| c.len()).min().unwrap_or(0);

    let mut prefix_len = 0;
    'prefix: for i in 0..min_len {
        let c0 = chars[0][i];
        for row in &chars {
            if row[i] != c0 {
                break 'prefix;
            }
        }
        prefix_len += 1;
    }

    let mut suffix_len = 0;
    'suffix: for i in 0..min_len {
        let c0 = chars[0][chars[0].len() - 1 - i];
        for row in &chars {
            if row[row.len() - 1 - i] != c0 {
                break 'suffix;
            }
        }
        suffix_len += 1;
    }

    // Trim overlap when prefix + suffix would exceed the shortest string
    // (matches the whole-string-equal case in the Python version).
    if prefix_len + suffix_len > chars[0].len() {
        suffix_len = chars[0].len() - prefix_len;
    }

    let prefix: String = chars[0][..prefix_len].iter().collect();
    let suffix: String = chars[0][chars[0].len() - suffix_len..].iter().collect();
    format!("{prefix}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_name_hash_and_slash() {
        assert_eq!(local_name("http://example.com#Foo"), "Foo");
        assert_eq!(local_name("http://example.com/Foo"), "Foo");
        assert_eq!(local_name("Foo"), "Foo");
    }

    #[test]
    fn common_pattern_prefix_suffix() {
        assert_eq!(
            common_pattern(&["urn:ex#sensor_1", "urn:ex#sensor_2"]),
            "urn:ex#sensor_"
        );
        assert_eq!(common_pattern(&["abc"]), "abc");
    }
}
