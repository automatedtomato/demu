// Terminal output sanitization.
//
// Strips control characters from strings before they are written to the
// terminal. This prevents escape-sequence injection when user-controlled data
// (Dockerfile instruction text, file paths, env var values) is echoed back to
// the user.
//
// Used by both the REPL layer (repl/mod.rs, repl/custom/) and the binary
// entrypoint (main.rs) so it lives here in `output` rather than under `repl`.

/// Strip terminal-unsafe characters from a string before printing.
///
/// Removes:
/// - C0 control characters: U+0000–U+001F (includes ESC, NUL, CR, LF, TAB)
/// - DEL: U+007F
/// - C1 control characters: U+0080–U+009F (includes CSI U+009B, which some
///   terminal emulators treat as an ANSI escape sequence introducer)
///
/// This prevents terminal escape injection when echoing user-supplied input
/// such as raw Dockerfile instruction text or file paths.
pub fn sanitize_for_terminal(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            let cp = c as u32;
            // Allow printable ASCII and all codepoints above the C1 range.
            !(cp <= 0x1F || cp == 0x7F || (0x80..=0x9F).contains(&cp))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_allows_printable_ascii() {
        assert_eq!(sanitize_for_terminal("hello world"), "hello world");
    }

    #[test]
    fn sanitize_strips_escape_sequences() {
        // ESC (0x1B) is a C0 control character.
        assert_eq!(sanitize_for_terminal("\x1b[2J"), "[2J");
    }

    #[test]
    fn sanitize_strips_del() {
        assert_eq!(sanitize_for_terminal("ab\x7Fcd"), "abcd");
    }

    #[test]
    fn sanitize_strips_c1_range() {
        // U+009B (CSI) encoded as UTF-8: 0xC2 0x9B — hits the 0x80..=0x9F range.
        assert_eq!(sanitize_for_terminal("\u{009B}"), "");
    }

    #[test]
    fn sanitize_preserves_unicode_above_c1() {
        // Japanese text well above U+009F must pass through unchanged.
        assert_eq!(sanitize_for_terminal("日本語"), "日本語");
    }

    #[test]
    fn sanitize_strips_nul() {
        // NUL (0x00) is C0 and a common binary injection byte.
        assert_eq!(sanitize_for_terminal("ab\x00cd"), "abcd");
    }

    #[test]
    fn sanitize_strips_cr() {
        // CR (0x0D) is C0 and can be used to overwrite terminal output.
        assert_eq!(sanitize_for_terminal("ab\rcd"), "abcd");
    }

    #[test]
    fn sanitize_empty_string_is_empty() {
        assert_eq!(sanitize_for_terminal(""), "");
    }
}
