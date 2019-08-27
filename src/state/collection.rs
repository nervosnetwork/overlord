use std::collections::{BTreeMap, HashMap};

use crate::types::{
    Address, AggregatedVote, Hash, Signature, SignedProposal, SignedVote, VoteType,
};
use crate::{error::ConsensusError, Codec, ConsensusResult};

///
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalCollector<T: Codec>(BTreeMap<u64, ProposalRoundCollector<T>>);

impl<T> ProposalCollector<T>
where
    T: Codec + Send + Sync,
{
    pub fn new() -> Self {
        ProposalCollector(BTreeMap::new())
    }

    pub fn insert(
        &mut self,
        epoch_id: u64,
        round: u64,
        proposal: SignedProposal<T>,
    ) -> ConsensusResult<()> {
        self.0
            .entry(epoch_id)
            .or_insert_with(ProposalRoundCollector::new)
            .insert(round, proposal)
            .map_err(|_| ConsensusError::MultiProposal(epoch_id, round))?;
        Ok(())
    }

    pub fn flush(&mut self, till: u64) {
        self.0.split_off(&till);
    }
}

///
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProposalRoundCollector<T: Codec>(HashMap<u64, SignedProposal<T>>);

impl<T> ProposalRoundCollector<T>
where
    T: Codec + Send + Sync,
{
    fn new() -> Self {
        ProposalRoundCollector(HashMap::new())
    }

    fn insert(&mut self, round: u64, proposal: SignedProposal<T>) -> ConsensusResult<()> {
        if self.0.get(&round).is_some() {
            return Err(ConsensusError::Other("_".to_string()));
        }
        self.insert(round, proposal);
        Ok(())
    }
}
