//! The platform extension points of Kubuno Whiteboard (the same shape as Documents' and Diagrams'): each
//! has a portable default; an OS folder overrides only what it does better, in the [`Platform`] its entry
//! point hands to [`crate::app::run`].

use std::sync::OnceLock;

/// How the command line is read where a system adds its own rules.
pub trait LaunchRules: Send + Sync {
    /// Whether the title bar's header shows the offline sample. `args` includes the program name.
    fn sample_requested(&self, args: &[String]) -> bool {
        args.iter().skip(1).any(|a| a == "--sample")
    }
}

/// The user interface: runs the app for these options until it closes, and returns the exit code.
pub trait UiHost {
    fn run(&self, options: &crate::app::Options) -> i32;
}

/// The portable defaults of every extension point.
#[derive(Debug, Default, Clone, Copy)]
pub struct Portable;

impl LaunchRules for Portable {}

/// The extension points an entry point registers.
pub struct Platform {
    /// The system's name, for the logs.
    pub name: &'static str,
    pub launch: Box<dyn LaunchRules>,
}

impl Platform {
    pub fn portable() -> Platform {
        Platform { name: "portable", launch: Box::new(Portable) }
    }
}

impl Default for Platform {
    fn default() -> Self {
        Platform::portable()
    }
}

static CURRENT: OnceLock<Platform> = OnceLock::new();

/// Registers the platform of this process (once).
pub fn install(platform: Platform) -> bool {
    CURRENT.set(platform).is_ok()
}

/// The registered platform, or the portable defaults.
pub fn current() -> &'static Platform {
    CURRENT.get_or_init(Platform::portable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_portable_rules_read_the_sample_switch_after_the_program_name() {
        let args = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(Portable.sample_requested(&args(&["kubuno-whiteboard", "--sample"])));
        assert!(!Portable.sample_requested(&args(&["--sample"])));
    }
}
