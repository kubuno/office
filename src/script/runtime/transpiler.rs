/// Simple TypeScript type stripper (no AST parser).
///
/// Removes:
/// - Lines starting with `import type`
/// - Lines matching `import { ... } from "@kubuno/script-api"` (Kubuno API is global)
/// - Inline type annotations  `: TypeName` and generic parameters `<Generic>`
///
/// This is intentionally minimal — sufficient for simple Kubuno scripts.
pub fn strip_typescript(code: &str) -> String {
    let mut output = String::with_capacity(code.len());

    for line in code.lines() {
        let trimmed = line.trim_start();

        // Drop `import type ...`
        if trimmed.starts_with("import type ") {
            output.push('\n');
            continue;
        }

        // Drop `import { ... } from "@kubuno/script-api"` — Kubuno API is global
        if trimmed.starts_with("import ") && trimmed.contains("from \"@kubuno/script-api\"") {
            output.push('\n');
            continue;
        }
        if trimmed.starts_with("import ") && trimmed.contains("from '@kubuno/script-api'") {
            output.push('\n');
            continue;
        }

        // Strip inline type annotations and generics from the line
        let stripped = strip_type_annotations(line);
        output.push_str(&stripped);
        output.push('\n');
    }

    output
}

/// Removes `: TypeName` annotations and `<Generic>` type parameters from a single line.
/// Uses a simple state machine — not a full parser.
fn strip_type_annotations(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    // Track string literal state to avoid mangling string contents
    let mut in_string: Option<char> = None;
    let mut escape_next = false;

    while i < len {
        let c = chars[i];

        if escape_next {
            result.push(c);
            escape_next = false;
            i += 1;
            continue;
        }

        if c == '\\' && in_string.is_some() {
            result.push(c);
            escape_next = true;
            i += 1;
            continue;
        }

        // Entering/leaving string literals
        if c == '"' || c == '\'' || c == '`' {
            if let Some(delim) = in_string {
                if delim == c {
                    in_string = None;
                }
            } else {
                in_string = Some(c);
            }
            result.push(c);
            i += 1;
            continue;
        }

        if in_string.is_some() {
            result.push(c);
            i += 1;
            continue;
        }

        // Strip `: TypeAnnotation` — colon followed by type
        // Only strip if we're not inside object literals (heuristic)
        if c == ':' {
            // Look ahead: skip whitespace then check if it looks like a type
            let rest = &chars[i + 1..];
            if looks_like_type_annotation(rest) {
                // Skip the colon and everything that belongs to the type
                i = skip_type_annotation(&chars, i + 1);
                continue;
            }
        }

        // Strip generic type parameters `<Type>` after identifiers
        if c == '<' && i > 0 {
            let prev = chars[i - 1];
            if prev.is_alphanumeric() || prev == '_' {
                // Could be a generic — try to skip balanced <>
                if let Some(end) = find_balanced_angle(&chars, i) {
                    // Check it doesn't look like comparison operators
                    i = end + 1;
                    continue;
                }
            }
        }

        result.push(c);
        i += 1;
    }

    result
}

/// Heuristically determine if what follows `:` is a type annotation.
fn looks_like_type_annotation(rest: &[char]) -> bool {
    // Skip whitespace
    let mut j = 0;
    while j < rest.len() && rest[j] == ' ' {
        j += 1;
    }
    if j >= rest.len() {
        return false;
    }
    let c = rest[j];
    // Type annotations start with uppercase letter, lowercase type keywords, or `{`, `[`, `(`
    c.is_uppercase()
        || matches!(c, '{' | '[' | '(')
        || is_type_keyword(rest, j)
}

fn is_type_keyword(chars: &[char], start: usize) -> bool {
    let keywords = ["string", "number", "boolean", "void", "null", "undefined", "any", "never", "unknown", "object"];
    for kw in &keywords {
        let kw_chars: Vec<char> = kw.chars().collect();
        if chars.len() >= start + kw_chars.len() {
            let slice = &chars[start..start + kw_chars.len()];
            if slice == kw_chars.as_slice() {
                // Make sure it's not part of a longer identifier
                let next = start + kw_chars.len();
                if next >= chars.len() || !chars[next].is_alphanumeric() {
                    return true;
                }
            }
        }
    }
    false
}

/// Skip over a type annotation (everything after the colon until we hit `=`, `,`, `)`, `{`, `;`, or newline).
fn skip_type_annotation(chars: &[char], start: usize) -> usize {
    let mut i = start;
    let mut depth = 0i32;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '(' | '[' | '{' | '<' => {
                depth += 1;
                i += 1;
            }
            ')' | ']' | '}' | '>' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                i += 1;
            }
            '=' | ',' | ';' if depth == 0 => break,
            _ => i += 1,
        }
    }
    i
}

/// Find the matching `>` for a `<` at position `start`, respecting nesting.
/// Returns None if it looks like comparison operators (e.g., `a < b > c`).
fn find_balanced_angle(chars: &[char], start: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = start;
    // Heuristic: if there's a space right after `<`, it's a comparison, not generic
    if i + 1 < chars.len() && chars[i + 1] == ' ' {
        return None;
    }
    while i < chars.len() {
        match chars[i] {
            '<' => {
                depth += 1;
                i += 1;
            }
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
                i += 1;
            }
            ',' | '\n' | ';' | '=' | '!' | '&' | '|' if depth == 1 => {
                // Might be comparison — abort
                if chars[i] == ',' {
                    i += 1;
                } else {
                    return None;
                }
            }
            _ => i += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_import_type() {
        let code = "import type { Foo } from './foo'\nconst x = 1;";
        let result = strip_typescript(code);
        assert!(!result.contains("import type"));
        assert!(result.contains("const x = 1;"));
    }

    #[test]
    fn strips_kubuno_import() {
        let code = "import { Kubuno } from \"@kubuno/script-api\"\nconst x = 2;";
        let result = strip_typescript(code);
        assert!(!result.contains("@kubuno/script-api"));
    }

    #[test]
    fn passthrough_plain_js() {
        let code = "const x = 42;\nconsole.log(x);";
        let result = strip_typescript(code);
        assert!(result.contains("const x = 42;"));
        assert!(result.contains("console.log(x);"));
    }
}
