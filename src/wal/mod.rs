#[cfg(test)]
mod tests;
mod wal_type;

pub use self::wal_type::{SMRBase, WalInfo, WalLock};
