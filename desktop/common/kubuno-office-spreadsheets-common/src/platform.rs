//! The platform extension points of Kubuno Spreadsheets.
//!
//! Everything the app does is written once, in `desktop/common`; what only an operating system can do goes
//! through the traits below, each with a portable default, so the app builds and runs on any system with
//! nothing registered. An OS folder overrides only what it does better, in the [`Platform`] its entry point
//! hands to [`crate::app::run`].
//!
//! | Extension point | Portable default | Windows override (`desktop/windows`) |
//! |---|---|---|
//! | [`LaunchRules`] | `--sample` asks for the offline sample header | also a Debug build under a debugger without `--live` |
//! | [`Clock`] | UTC from the system time (no zone database in the portable crates) | the local time (`GetLocalTime`) |
//! | [`UiHost`] | the active sheet as text ([`crate::app::TextUi`]) | the Win32 window |

use std::sync::OnceLock;

use kubuno_office_sheets_core::jsdate::LocalNow;

/// How the command line is read where a system adds its own rules.
pub trait LaunchRules: Send + Sync {
    /// Whether the title bar's header shows the offline sample instead of the account's data. `args`
    /// includes the program name.
    fn sample_requested(&self, args: &[String]) -> bool {
        args.iter().skip(1).any(|a| a == "--sample")
    }
}

/// The local wall-clock time, for `NOW()`, `TODAY()` and the date conditional formats (the web reads the
/// browser's local time).
pub trait Clock: Send + Sync {
    fn now(&self) -> LocalNow {
        utc_now()
    }
}

/// The current UTC time as wall-clock components.
pub fn utc_now() -> LocalNow {
    let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let ms = d.as_millis() as f64;
    let p = kubuno_office_sheets_core::jsdate::parts(ms);
    match p {
        Some(p) => LocalNow {
            year: p.year as i32,
            month: p.month as u32 + 1,
            day: p.date as u32,
            hour: p.hours as u32,
            minute: p.minutes as u32,
            second: p.seconds as u32,
            millis: p.millis as u32,
        },
        None => LocalNow { year: 1970, month: 1, day: 1, ..LocalNow::default() },
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

/// What formulas see: the platform's clock and a random sequence seeded at start.
pub struct SystemEnv {
    seed: std::cell::Cell<u64>,
}

impl Default for SystemEnv {
    fn default() -> Self {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        Self { seed: std::cell::Cell::new((t.as_nanos() as u64) | 1) }
    }
}

impl kubuno_office_sheets_core::formula::Env for SystemEnv {
    fn now(&self) -> LocalNow {
        current().clock.now()
    }

    fn random(&self) -> f64 {
        // xorshift64*: Math.random's contract (uniform in [0, 1)), not its sequence.
        let mut x = self.seed.get();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.seed.set(x);
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_portable_rules_read_the_sample_switch_after_the_program_name() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(Portable.sample_requested(&args(&["kubuno-spreadsheets", "--sample"])));
        assert!(!Portable.sample_requested(&args(&["--sample"])));
    }

    #[test]
    fn the_portable_clock_is_a_valid_date() {
        let n = Portable.now();
        assert!(n.year >= 2024 && (1..=12).contains(&n.month) && (1..=31).contains(&n.day));
        let e = SystemEnv::default();
        let r = kubuno_office_sheets_core::formula::Env::random(&e);
        assert!((0.0..1.0).contains(&r));
    }
}
