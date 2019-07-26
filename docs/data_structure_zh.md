# Muta consensus 数据结构

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
pub struct SignedProposal<P> {
    pub signature: Vec<u8>,
    pub proposal: Proposal<P>,
}

pub struct Proposal<P> {
    pub height: u64,
    pub round: u64,
    pub content: P,
    pub lock_round: Option<u64>,
    pub lock_votes: Vec<AggregatedVote<P>>,
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
    pub height: u64,
    pub round: u64,
    pub proposal: Hash,
}

pub struct Vote {
    pub type: VoteType,
    pub height: u64,
    pub round: u64,
    pub proposal: Hash,
    pub voter: Address,
}
```

## Commit

```rust
pub struct Commit<P, T> {
    pub height: u64,
    pub proposal: P,
    pub txs_body: T,
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
    pub height: u64,
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
pub Status {
    pub height: u64,
    pub interval: u64,
    pub authority_list: Vec<Node>,
}
```

## VerifyResp

```rust
pub(crate) struct VerifyResp<T> {
    pub(crate) proposal_hash: Hash,
    pub(crate) is_pass: bool,
    pub(crate) txs_body: T,
}
```

## Feed

```rust
pub(crate) struct Feed<P> {
    pub(crate) height: u64,
    pub(crate) proposal: P,
    pub(crate) hash: Hash,
}
```