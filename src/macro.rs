/// log with prefix
#[macro_export]
macro_rules! error {
    ($($arg:tt)+) => (
        let fmt = format!($($arg)+);
        log::error!("{} {}", crate::LOG_PREFIX.read(), fmt);
    )
}

/// log with prefix
#[macro_export]
macro_rules! warn {
    ($($arg:tt)+) => (
        let fmt = format!($($arg)+);
        log::warn!("{} {}", crate::LOG_PREFIX.read(), fmt);
    )
}

/// log with prefix
#[macro_export]
macro_rules! debug {
    ($($arg:tt)+) => (
        let fmt = format!($($arg)+);
        log::debug!("{} {}", crate::LOG_PREFIX.read(), fmt);
    )
}

/// log with prefix
#[macro_export]
macro_rules! info {
    ($($arg:tt)+) => (
        let fmt = format!($($arg)+);
        log::info!("{} {}", crate::LOG_PREFIX.read(), fmt);
    )
}
