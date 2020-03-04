mod crypto;
mod primitive;
mod run;
mod utils;
mod wal;

// use std::fs;

use run::run_test;
use wal::Record;

const TEST_CASE_DIR: &str = "./tests/integration_tests/test_case/";

#[tokio::test(threaded_scheduler)]
async fn test_1_wal() {
    run_test(Record::new(1, 100), 1, 10).await
}

#[tokio::test(threaded_scheduler)]
async fn test_3_wal() {
    run_test(Record::new(3, 100), 1, 10).await
}

// #[tokio::test(threaded_scheduler)]
// async fn test_4_wal() {
//     run_test(Record::new(4, 100), 1, 10).await
// }

// #[tokio::test(threaded_scheduler)]
// async fn test_21_wal() {
//     // let _ = env_logger::builder().is_test(true).try_init();
//     run_test(Record::new(21, 1), 1, 10).await
// }

// #[tokio::test(threaded_scheduler)]
// async fn test_case() {
//     let test_file = "./tests/intergrate_tests/test_case/case1.json";
//     run_test(Record::load(test_file), 10, 10).await
// }

// #[tokio::test(threaded_scheduler)]
// async fn test_cases() {
//     for entry in fs::read_dir(TEST_CASE_DIR).unwrap() {
//         let path = entry.unwrap().path();
//         run_test(Record::load(&path.display().to_string()), 10, 10).await
//     }
// }
