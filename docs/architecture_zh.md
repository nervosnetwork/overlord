# Muta Consensus 架构设计

## 组件

* 共识状态机
* wal
* DB
* utils

### 共识状态机

使用一个 BLS 优化的 Tendermint 状态机对 proposal 进行共识。 当 leader 掉线时，退化到传统的 Tendermint 状态机。

### Wal

异步写二进制文件。

### DB

Epoch 共识成功后，写入 rocksDB 数据库。

## 接口

### Consensus API

```rust
pub trait Consensus<T: Serialize + Deserialize + Clone + Debug> {
    /// Consensus error
    type Error: Debug;
    /// Get an epoch of an epoch_id and return the epoch with its hash.
    fn get_epoch(&self, epoch_id: u64) -> Future<Output = Result<(T, Hash)), Self::Error>>;
    /// Check the correctness of an epoch.
    fn check_epoch(&self, hash: &[u8]) -> Future<Output = Result<(), Self::Error>>;
    /// Commit an epoch.
    fn commit(&self, epoch_id: u64, commit: Commit<T>) -> Future<Output = Result<(), Self::Error>>;
    /// Transmit a message to the leader.
    fn transmit_to_leader(&self, addr: Address, msg: OutputMsg) -> Future<Output = Result<(), Self::Error>>;
    /// Broadcast a message to other replicas.
    fn broadcast_to_other(&self, msg: OutputMsg) -> Future<Output = Result<(), Self::Error>>;
}
```

### Crypto API

```rust
pub trait Crypto {
    /// Crypto error.
    type Error: Debug;
    /// Crypt hash.
    fn hash(&self, msg: &[u8) -> Hash;
    /// Sign to a hash with private key.
    fn sign(&self, hash: &[u8]) -> Result<Signature, Self::Error>;
    /// Crypt aggregate signature from signatures.
    fn aggregate_sign(&self, signatures: Vec<Signatures>) -> Result<Signature, Self::Error>;
    /// Verify a signature.
    fn verify_signature(&self, signature: &[u8], hash: &[u8]) -> Result<Address, Self::Error>;
    /// Verify an aggregate signature.
    fn verify_aggregate_signature(&self, aggregate_signature: Signature) -> Result<(), Self::Error>;
}
```