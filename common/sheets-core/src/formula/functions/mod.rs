//! The function registry (`FUNCTIONS` of `formula-engine.ts`): one module per file of the web's
//! `formula-fns/`, merged in the web's order, the engine's own block last (it wins on a shared name).

use std::collections::HashMap;
use std::sync::OnceLock;

use super::eval::Fn;

pub mod base;
pub mod date;
pub mod engineering;
pub mod engineering2;
pub mod financial;
pub mod financial2;
pub mod logical;
pub mod lookup;
pub mod math;
pub mod misc;
pub mod stat;
pub mod stat2;
pub mod text;

/// Name → implementation.
pub type Registry = HashMap<&'static str, Fn>;

/// The registry, built once.
pub fn registry() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(|| {
        let mut r = Registry::new();
        // `...MATH_FNS, ...STAT_FNS, ...TEXT_FNS, ...DATE_FNS, ...LOGICAL_FNS, ...LOOKUP_FNS,
        //  ...FINANCIAL_FNS, ...ENGINEERING_FNS, ...STAT2_FNS, ...FINANCIAL2_FNS, ...ENGINEERING2_FNS,
        //  ...MISC_FNS`, then the engine's block.
        math::register(&mut r);
        stat::register(&mut r);
        text::register(&mut r);
        date::register(&mut r);
        logical::register(&mut r);
        lookup::register(&mut r);
        financial::register(&mut r);
        engineering::register(&mut r);
        stat2::register(&mut r);
        financial2::register(&mut r);
        engineering2::register(&mut r);
        misc::register(&mut r);
        base::register(&mut r);
        r
    })
}

/// `KNOWN_FUNCTIONS`: every registered name, sorted.
pub fn known_functions() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = registry().keys().copied().collect();
    v.sort_unstable();
    v
}
