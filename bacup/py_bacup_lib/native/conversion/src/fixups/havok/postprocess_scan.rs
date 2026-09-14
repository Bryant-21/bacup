pub(crate) fn contains_ascii_case_insensitive(data: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && data.windows(needle.len()).any(|window| {
            window
                .iter()
                .zip(needle)
                .all(|(actual, expected)| actual.eq_ignore_ascii_case(expected))
        })
}

pub(crate) fn contains_animation_class(data: &[u8]) -> bool {
    contains_ascii_case_insensitive(data, b"hkaSplineCompressedAnimation")
        || contains_ascii_case_insensitive(data, b"hkaInterleavedUncompressedAnimation")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_mixed_case_ascii_token() {
        assert!(contains_ascii_case_insensitive(
            b"prefix Fo76_WeaponAttackStart suffix",
            b"fo76_"
        ));
    }

    #[test]
    fn rejects_partial_and_empty_tokens() {
        assert!(!contains_ascii_case_insensitive(b"fo76", b"fo76_"));
        assert!(!contains_ascii_case_insensitive(b"anything", b""));
    }

    #[test]
    fn recognizes_both_mutable_animation_classes() {
        assert!(contains_animation_class(b"hkaSplineCompressedAnimation"));
        assert!(contains_animation_class(
            b"hkaInterleavedUncompressedAnimation"
        ));
        assert!(!contains_animation_class(b"hkbBehaviorGraph"));
    }
}
