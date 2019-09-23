# Overlord

[![CircleCI](https://circleci.com/gh/cryptape/overlord.svg?style=svg)](https://circleci.com/gh/cryptape/overlord)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE.md)
[![Minimum rustc version](https://img.shields.io/badge/rustc-1.39+-informational.svg)](https://github.com/cryptape/overlord/blob/master/rust-toolchain)

Overlord consensus protocol.

## Intrucduction

Overlord is a new consensus protocol that decouple the consensus process from the execution process.

Detaild intrucduction: [中文](./docs/architecture_zh.md)|English

## Usage

### From cargo

```toml
[dependencies]
overlord = { git = "https://github.com/cryptape/overlord.git" }
```

### Example

```rust
use overlord::{Codec, Consensus, Context, Crypto, Overlord, OverlordHandler};

impl<T, F> Consensus<T, F> for OverlordEngine {}

impl Crypto for OverlordCrypto {}

fn main() {
    let overlord = Overlord::new(address, OverlordEngine, OverlordCrypto);
    let handler = overlord.get_handler();

    tokio::run(overlord.run());
    handler.send_msg().unwrap();
}
```
