# Overlord

Overlord consensus protocol.

[![Crates.io](https://img.shields.io/crates/v/overlord)](https://crates.io/crates/overlord)
![example workflow](https://github.com/nervosnetwork/overlord/actions/workflows/ci.yml/badge.svg)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE.md)
[![Minimum rustc version](https://img.shields.io/badge/rustc-1.58.1+-informational.svg)](https://github.com/nervosnetwork/overlord/blob/master/rust-toolchain)

## Introduction

Overlord is a new consensus protocol that decouple the consensus process from the execution process.

Detailed introduction: [中文](./docs/architecture_zh.md)|[English](./docs/architecture_en.md)

## Usage

### From cargo

```toml
[dependencies]
overlord = "0.3"
```

Overlord takes turns to become the leader by default. If you want to choose a leader randomly, add the `random_leader` feature to the dependency as below.

```toml
[dependencies]
overlord = { version = "0.4", features = ["random_leader"] }
```

### Example

We simulated a salon scene to show an example of using overlord.

A distributed system for reaching a consensus on the content of a speech is realized by simulating the dialogue between speakers through the communication between threads.

Run the example by `cargo run --example salon`, and the system will output the agreed speech content in turn. Click [here](./examples/salon.rs) to see the detail.

It will check whether different speakers agree on the content of the speech.

### Projects using Overlord

* [Muta](https://github.com/nervosnetwork/muta), a high-performance blockchain framework.
* [Axon](https://github.com/nervosnetwork/axon), a layer2 of CKB that is compatible with Ethereum.
