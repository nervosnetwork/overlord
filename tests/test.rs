mod common;

use crate::common::platform::Platform;

#[tokio::test(threaded_scheduler)]
async fn test_4_nodes() {
    Platform::new(4);
}
