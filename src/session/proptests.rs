use super::*;

proptest::proptest! {
    #[test]
    fn validate_session_id_never_panics(s in ".*") {
        let _ = validate_session_id(&s);
    }

    #[test]
    fn validate_session_id_accepts_valid_chars_only(s in "[a-zA-Z0-9_-]{1,128}") {
        proptest::prop_assert!(validate_session_id(&s).is_ok(), "valid session_id '{}' should pass", s);
    }

    #[test]
    fn validate_snapshot_name_rejects_path_traversal(s in ".*") {
        let result = validate_snapshot_name(&s);
        if s.contains('/') || s.contains('\\') || s.contains("..") {
            proptest::prop_assert!(result.is_err(), "snapshot name '{}' with path traversal should fail", s);
        }
    }

    #[test]
    fn cleanup_timestamp_rfc3339_never_panics(ts in "[0-9T:+ -Zz.]{1,40}") {
        let _ = chrono::DateTime::parse_from_rfc3339(&ts);
    }
}
