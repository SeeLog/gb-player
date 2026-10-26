//! Byte patterns with wildcards, used to locate sound engine routines in a ROM without
//! hard-coding addresses (which differ between versions and languages).

pub struct Pattern(Vec<Option<u8>>);

impl Pattern {
    /// Parse a pattern such as `"47 EA ?? ?? AF"`; `??` matches any byte.
    pub fn new(s: &str) -> Pattern {
        Pattern(
            s.split_whitespace()
                .map(|t| {
                    if t == "??" {
                        None
                    } else {
                        Some(u8::from_str_radix(t, 16).expect("bad pattern byte"))
                    }
                })
                .collect(),
        )
    }

    pub fn matches_at(&self, data: &[u8], pos: usize) -> bool {
        data.len() >= pos + self.0.len()
            && self
                .0
                .iter()
                .zip(&data[pos..])
                .all(|(p, &b)| p.is_none_or(|p| p == b))
    }

    pub fn find_all<'a>(&'a self, data: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
        (0..data.len().saturating_sub(self.0.len() - 1)).filter(move |&i| self.matches_at(data, i))
    }
}

pub fn word(data: &[u8], pos: usize) -> u16 {
    u16::from_le_bytes([data[pos], data[pos + 1]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_match() {
        let p = Pattern::new("47 EA ?? ?? AF");
        let data = [0x00, 0x47, 0xEA, 0x12, 0x34, 0xAF, 0x47, 0xEA];
        assert_eq!(p.find_all(&data).collect::<Vec<_>>(), vec![1]);
        assert_eq!(word(&data, 3), 0x3412);
    }
}
