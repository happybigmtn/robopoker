//! STW-044: per-arm error-shape audit.
//!
//! Static-grep tests that pin the `live_proof ...` error-line
//! shapes across the autotrain source files. A refactor that
//! deletes or renames a pinned prefix fails CI on the next run.
//!
//! No production code — `cfg(test)` only.

#[cfg(test)]
mod tests {
    use std::fs;

    fn read_source(name: &str) -> String {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join(name);
        fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e))
    }

    // 1. publish.rs — ReceiptRed is the canonical publish error arm
    #[test]
    fn publish_receipt_red_error_line_uses_pinned_shape() {
        let src = read_source("publish.rs");
        assert!(
            src.contains("live_proof publish error: receipt is red:"),
            "publish.rs must contain the pinned receipt-red error prefix"
        );
    }

    // 2. publish_remote.rs — ReceiptRed is the canonical publish_remote error arm
    #[test]
    fn publish_remote_receipt_red_error_line_uses_pinned_shape() {
        let src = read_source("publish_remote.rs");
        assert!(
            src.contains("live_proof publish_remote error: receipt is red:"),
            "publish_remote.rs must contain the pinned receipt-red error prefix"
        );
    }

    // 3. publish_index.rs — RemoteReceiptRed is the canonical publish_index error arm
    #[test]
    fn publish_index_remote_receipt_red_error_line_uses_pinned_shape() {
        let src = read_source("publish_index.rs");
        assert!(
            src.contains("live_proof publish_index error: remote receipt is red:"),
            "publish_index.rs must contain the pinned remote-receipt-red error prefix"
        );
    }

    // 4. publish_index_remote.rs — IndexRed is the canonical publish_index_remote error arm
    #[test]
    fn publish_index_remote_index_red_error_line_uses_pinned_shape() {
        let src = read_source("publish_index_remote.rs");
        assert!(
            src.contains("live_proof publish_index_remote error: index is red:"),
            "publish_index_remote.rs must contain the pinned index-red error prefix"
        );
    }

    // 5. verify_receipt.rs — recipe_shape failure
    #[test]
    fn verify_receipt_recipe_shape_failed_line_uses_pinned_shape() {
        let src = read_source("verify_receipt.rs");
        assert!(
            src.contains("live_proof receipt verification failed: recipe_shape:"),
            "verify_receipt.rs must contain the pinned recipe-shape failure prefix"
        );
    }

    // 6. verify_receipt.rs — passed
    #[test]
    fn verify_receipt_passed_line_uses_pinned_shape() {
        let src = read_source("verify_receipt.rs");
        assert!(
            src.contains("live_proof receipt verification passed:"),
            "verify_receipt.rs must contain the pinned verification-passed prefix"
        );
    }

    // 7. verify_receipt.rs — generic failed
    #[test]
    fn verify_receipt_failed_line_uses_pinned_shape() {
        let src = read_source("verify_receipt.rs");
        assert!(
            src.contains("live_proof receipt verification failed: {kind}: {detail}"),
            "verify_receipt.rs must contain the pinned generic verification-failed prefix"
        );
    }

    // 8. verify_bundle.rs — passed
    #[test]
    fn verify_bundle_passed_line_uses_pinned_shape() {
        let src = read_source("verify_bundle.rs");
        assert!(
            src.contains("live_proof bundle verification passed:"),
            "verify_bundle.rs must contain the pinned bundle-verification-passed prefix"
        );
    }

    // 9. verify_bundle.rs — failed
    #[test]
    fn verify_bundle_failed_line_uses_pinned_shape() {
        let src = read_source("verify_bundle.rs");
        assert!(
            src.contains("live_proof bundle verification failed:"),
            "verify_bundle.rs must contain the pinned bundle-verification-failed prefix"
        );
    }

    // 10. mode.rs — the --error-shape-test flag (surviving piece of STW-038)
    #[test]
    fn mode_has_error_shape_test_flag() {
        let src = read_source("mode.rs");
        assert!(
            src.contains("--error-shape-test"),
            "mode.rs must contain the --error-shape-test argv flag"
        );
    }

    // 11. STW-051: publish_index.rs — MissingArg is the new arm
    #[test]
    fn publish_index_missing_arg_error_line_uses_pinned_shape() {
        let src = read_source("publish_index.rs");
        assert!(
            src.contains("live_proof publish_index error: missing arg:"),
            "publish_index.rs must contain the pinned missing-arg error prefix"
        );
    }
}
