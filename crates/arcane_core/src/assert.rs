//! Debug assertions with structured failure metadata. These compile out in
//! release builds — see [`cfg!(debug_assertions)`].

/// Like `assert!` but tags the failure as an Arcane-engine invariant. Adds
/// no runtime overhead in release builds.
#[macro_export]
macro_rules! arc_assert {
    ($cond:expr $(,)?) => {
        #[cfg(debug_assertions)]
        assert!($cond, "arcane invariant violated");
        #[cfg(not(debug_assertions))]
        let _ = (|| { let _ = $cond; })();
    };
    ($cond:expr, $msg:literal $(,)?) => {
        #[cfg(debug_assertions)]
        assert!($cond, "arcane invariant: {}", $msg);
        #[cfg(not(debug_assertions))]
        let _ = (|| { let _ = $cond; })();
    };
    ($cond:expr, $fmt:expr, $($arg:tt)*) => {
        #[cfg(debug_assertions)]
        assert!($cond, "arcane invariant: {}", format!($fmt, $($arg)*));
        #[cfg(not(debug_assertions))]
        let _ = (|| { let _ = $cond; })();
    };
}

/// Like `assert_eq!` but tags the failure as an Arcane-engine invariant.
#[macro_export]
macro_rules! arc_assert_eq {
    ($l:expr, $r:expr $(,)?) => {
        #[cfg(debug_assertions)]
        assert_eq!($l, $r, "arcane invariant (eq) violated");
        #[cfg(not(debug_assertions))]
        let _ = (|| { let _ = ($l, $r); })();
    };
    ($l:expr, $r:expr, $msg:literal $(,)?) => {
        #[cfg(debug_assertions)]
        assert_eq!($l, $r, "arcane invariant: {}", $msg);
        #[cfg(not(debug_assertions))]
        let _ = (|| { let _ = ($l, $r); })();
    };
}

/// Like `assert_ne!` but tags the failure as an Arcane-engine invariant.
#[macro_export]
macro_rules! arc_assert_ne {
    ($l:expr, $r:expr $(,)?) => {
        #[cfg(debug_assertions)]
        assert_ne!($l, $r, "arcane invariant (ne) violated");
        #[cfg(not(debug_assertions))]
        let _ = (|| { let _ = ($l, $r); })();
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_assert_passes_for_true() {
        arc_assert!(2 + 2 == 4);
        arc_assert!(2 + 2 == 4, "math is broken");
        arc_assert!(2 + 2 == 4, "math: {} != {}", 4, 5);
    }

    #[test]
    fn arc_assert_eq_passes() {
        let a = 5;
        let b = 5;
        arc_assert_eq!(a, b);
        arc_assert_eq!(a, b, "a should equal b");
    }

    #[test]
    fn arc_assert_ne_passes() {
        let a = 5;
        let b = 6;
        arc_assert_ne!(a, b);
    }

    #[test]
    #[should_panic(expected = "arcane invariant")]
    #[cfg(debug_assertions)]
    fn arc_assert_panics_in_debug() {
        arc_assert!(false, "this should panic");
    }
}
