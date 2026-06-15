/// Helpers and constants for the JavaScript runtime API bridge.
/// The actual injection happens in sandbox.rs.

/// The TypeScript declaration for the Kubuno global namespace.
/// Returned by GET /script/api-types.
pub const KUBUNO_API_TYPES: &str = r#"
declare namespace Kubuno {
  namespace Utils {
    /** Replace {{key}} placeholders in text with values from vars */
    function template(text: string, vars: Record<string, unknown>): string;
    /** Format a date string/timestamp using a format string (default: ISO) */
    function formatDate(date: string | number, fmt?: string): string;
    /** Generate a random UUID v4 string */
    function generateId(): string;
  }

  namespace Script {
    /** Input properties passed to this script invocation */
    const props: Record<string, unknown>;
  }

  namespace Http {
    /** Perform an HTTP GET request and return the parsed JSON body */
    function get(url: string, headers?: Record<string, string>): unknown;
    /** Perform an HTTP POST request with JSON body */
    function post(url: string, body: unknown, headers?: Record<string, string>): unknown;
  }
}

declare namespace console {
  function log(...args: unknown[]): void;
  function warn(...args: unknown[]): void;
  function error(...args: unknown[]): void;
}
"#;
