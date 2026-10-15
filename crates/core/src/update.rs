//! Which release the updater may offer: only a plain `X.Y.Z` strictly newer
//! than the running version. Never a downgrade, the same version, a
//! prerelease or build tag, or anything that doesn't parse.

/// `X.Y.Z` with no leading zeros, signs, spaces or suffixes.
fn parse(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.split('.').map(|p| {
        let plain = !p.is_empty()
            && p.bytes().all(|b| b.is_ascii_digit())
            && (p == "0" || !p.starts_with('0'));
        plain.then(|| p.parse::<u64>().ok()).flatten()
    });
    let v = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(v)
}

/// True only if `offered` is a plain version strictly newer than `current`.
pub fn newer(current: &str, offered: &str) -> bool {
    matches!((parse(current), parse(offered)), (Some(c), Some(o)) if o > c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_plain_newer_version_is_offered() {
        for offered in ["0.8.1", "0.9.0", "1.0.0", "0.10.0"] {
            assert!(newer("0.8.0", offered), "{offered}");
        }
        for offered in [
            "0.8.0",
            "0.7.9",
            "0.0.1",
            "0.8.1-beta",
            "0.8.1+build",
            "v0.8.1",
            "0.8",
            "0.8.1.0",
            "00.9.0",
            "0.09.0",
            " 0.9.0",
            "0.9.0 ",
            "",
            "0.9.x",
            "-1.0.0",
            "1.0.0\n",
            "99999999999999999999.0.0",
        ] {
            assert!(!newer("0.8.0", offered), "{offered:?}");
        }
        // An odd current version offers nothing.
        assert!(!newer("0.8.0-dev", "0.9.0"));
    }
}
