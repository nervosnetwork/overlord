mod crypto;
mod primitive;
mod run;
mod utils;
mod wal;

use run::run_test;

#[tokio::test(threaded_scheduler)]
async fn test_1_wal() {
    run_test(1, 100, 1, 200).await
}

// #[tokio::test(threaded_scheduler)]
// async fn test_3_wal() {
//     run_wal_test(3, 100, 1, 20).await
// }

#[tokio::test(threaded_scheduler)]
async fn test_4_wal() {
    // let _ = env_logger::builder().is_test(true).try_init();
    run_test(4, 100, 1, 20).await
}

#[tokio::test(threaded_scheduler)]
async fn test_21_wal() {
    run_test(21, 5, 5, 200).await
}
