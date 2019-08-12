# Overlord 数据结构

## 类型

```rust
type Address = Vec<u8>;

type Signature = Vec<u8>;

type Hash = Vec<u8>;
```

## 枚举

```rust
pub enum Role {
    Leader = 0,
    Replica = 1,
}

pub enum VoteType {
    Prevote = 0,
    Precommit = 1,
}

pub enum OutputMsg {
    SignedProposal(SignedProposal),
    SignedVote(SignedVote),
    AggregatedVote(AggregatedVote),
}
```

## Proposal

```rust
pub struct SignedProposal<T> {
    pub signature: Vec<u8>,
    pub proposal: Proposal<T>,
}

pub struct Proposal<T> {
    pub epoch_id: u64,
    pub round: u64,
    pub hash: Hash,
    pub content: T,
    pub lock_round: Option<u64>,
    pub lock_votes: Vec<AggregatedVote<T>>,
    pub proposer: Address,
}
```

## Vote

```rust
pub struct SignedVote {
    pub signature: Vec<u8>,
    pub vote: Vote,
}

pub struct AggregatedVote {
    pub signature: AggregatedSignature,
    pub type: VoteType,
    pub epoch_id: u64,
    pub round: u64,
    pub proposal: Hash,
}

pub struct Vote {
    pub epoch_id: u64,
    pub round: u64,
    pub type: VoteType,
    pub proposal: Hash,
    pub voter: Address,
}
```

## Commit

```rust
pub struct Commit<T> {
    pub epoch_id: u64,
    pub proposal: T,
    pub proof: Proof,
}
```

## AggregatedSignature

```rust
pub struct AggregatedSignature {
    pub signature: Vec<u8>,
    pub address_bitmap: Vec<u8>,
}
```

## Proof

```rust
pub struct Proof {
    pub epoch_id: u64,
    pub round: u64,
    pub proposal_hash: Hash,
    pub signature: AggregatedSignature,
}
```

## Node

```rust
pub struct Node {
    pub address: Address,
    pub proposal_weight: usize,
    pub vote_weight: usize,
}
```

## Status

```rust
pub struct Status {
    pub epoch_id: u64,
    pub interval: u64,
    pub authority_list: Vec<Node>,
}
```

## VerifyResp

```rust
pub(crate) struct VerifyResp {
    pub(crate) proposal_hash: Hash,
    pub(crate) is_pass: bool,
}
```

## Feed

```rust
pub(crate) struct Feed<T> {
    pub(crate) epoch_id: u64,
    pub(crate) proposal: T,
    pub(crate) hash: Hash,
}
```
