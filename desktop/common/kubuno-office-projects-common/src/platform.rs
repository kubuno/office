//! The platform extension points of Kubuno Projects.
//!
//! Everything the app does is written once, in `desktop/common`; what only an operating system can do goes
//! through the traits below, each with a portable default, so the app builds and runs on any system with
//! nothing registered. An OS folder overrides only what it does better, in the [`Platform`] its entry point
//! hands to [`crate::app::run`].
//!
//! | Extension point | Portable default | Windows override (`desktop/windows`) |
//! |---|---|---|
//! | [`LaunchRules`] | `--sample` asks for the offline sample header | also a Debug build under a debugger without `--live` |
//! | [`Clock`] | UTC from the system time (no zone database in the portable crates) | the local date and time (`GetLocalTime`) |
//! | [`UiHost`] | the plan as text ([`crate::app::TextUi`]) | the Win32 window |

use std::sync::OnceLock;

use kubuno_office_projects_core::date::Date;

/// How the command line is read where a system adds its own rules.
pub trait LaunchRules: Send + Sync {
    /// Whether the title bar's header shows the offline sample instead of the account's data. `args`
    /// includes the program name.
    fn sample_requested(&self, args: &[String]) -> bool {
        args.iter().skip(1).any(|a| a == "--sample")
    }
}

/// The local calendar date (the web's `new Date()`: the project without a start date is anchored on today,
/// and the Gantt's today band) and the time in Unix milliseconds (gesture coalescing, creation stamps).
pub trait Clock: Send + Sync {
    fn today(&self) -> Date {
        Date::from_days(now_ms().div_euclid(86_400_000))
    }

    fn now_ms(&self) -> i64 {
        now_ms()
    }
}

/// The system time in Unix milliseconds.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// The user interface: runs the app for these options until it closes, and returns the exit code.
pub trait UiHost {
    fn run(&self, options: &crate::app::Options) -> i32;
}

/// The portable defaults of every extension point.
#[derive(Debug, Default, Clone, Copy)]
pub struct Portable;

impl LaunchRules for Portable {}
impl Clock for Portable {}

/// The extension points an entry point registers: the portable defaults, with what its OS overrides.
pub struct Platform {
    /// The system's name, for the logs (`windows`, `linux`, `macos`, `portable`).
    pub name: &'static str,
    pub launch: Box<dyn LaunchRules>,
    pub clock: Box<dyn Clock>,
}

impl Platform {
    /// Every extension point at its portable default.
    pub fn portable() -> Platform {
        Platform { name: "portable", launch: Box::new(Portable), clock: Box::new(Portable) }
    }
}

impl Default for Platform {
    fn default() -> Self {
        Platform::portable()
    }
}

static CURRENT: OnceLock<Platform> = OnceLock::new();

/// Registers the platform of this process (once; a second call is ignored and returns `false`).
pub fn install(platform: Platform) -> bool {
    CURRENT.set(platform).is_ok()
}

/// The registered platform, or the portable defaults when none was.
pub fn current() -> &'static Platform {
    CURRENT.get_or_init(Platform::portable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_portable_rules() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(Portable.sample_requested(&args(&["kubuno-projects", "--sample"])));
        assert!(!Portable.sample_requested(&args(&["--sample"])));
        let today = Portable.today();
        assert!(today.year >= 2024);
        assert!(Portable.now_ms() > 1_700_000_000_000);
    }
}
