// use std::collections::{BTreeMap, HashMap};
// use std::error::Error;
//
// use derive_more::Display;
//
// use crate::{Blk, Height, Round};
// use crate::types::{SignedProposal, SignedVote, AggregatedVote};
//
// pub enum CollectType {
//
// }
//
// pub struct HeightCollector<B: Blk>(BTreeMap<Height, RoundCollector<B>>);
//
// impl<B: Blk> HeightCollector<B> {
//     pub fn new() -> Self {
//         HeightCollector(BTreeMap::new())
//     }
//
//     pub fn insert(&mut self, signed_proposal: SignedProposal<B>) -> Result<(), CollectionError<B>> {
//         let height = signed_proposal.proposal.height;
//         let round = signed_proposal.proposal.round;
//         self.0
//             .entry(height)
//             .or_insert_with(SignedProposalRoundCollector::new)
//             .insert(round, signed_proposal)
//     }
//
//     pub fn get(&self, height: &Height, round: &Round) -> Option<&SignedProposal<B>> {
//         if let Some(round_collector) = self.0.get(height) {
//             round_collector.get(round)
//         } else {
//             None
//         }
//     }
//
//     pub fn pop(&mut self, height: &Height) -> Option<Vec<SignedProposal<B>>> {
//         self.0.remove(height).map_or_else(
//             || None,
//             |round_collector| Some(round_collector.0.values().cloned().collect())
//         )
//     }
//
//     pub fn remove_below(&mut self, height: &Height) {
//         self.0.split_off(height);
//     }
// }
//
// struct RoundCollector<B: Blk>(HashMap<Round, SignedProposal<B>>);
//
// impl<B: Blk> RoundCollector<B> {
//     fn new() -> Self {
//         RoundCollector(HashMap::new())
//     }
//
//     fn insert(&mut self, round: Round, signed_proposal: SignedProposal<B>) -> Result<(), CollectionError<B>> {
//         if let Some(exist_signed_proposal) = self.0.get(&round) {
//             if exist_signed_proposal == &signed_proposal {
//                 return Err(CollectionError::ProposalAlreadyExists(signed_proposal));
//             }
//             return Err(CollectionError::InsertDiffProposal(signed_proposal, exist_signed_proposal.clone()));
//         }
//         self.0.insert(round, signed_proposal);
//         Ok(())
//     }
//
//     fn get(&self, round: &Round) -> Option<&SignedProposal<B>> {
//         self.0.get(round)
//     }
// }
//
// pub struct Collector<B: Blk> {
//     signed_proposal: SignedProposal<B>,
//     signed_pre_votes: SignedVote,
//     signed_prcommit_votes: SignedVote,
//     aggregated_votes: AggregatedVote,
//
// }
//
// #[allow(dead_code)]
// #[derive(Debug, Display)]
// pub enum CollectionError<B: Blk> {
//     #[display(fmt = "signed_proposal: {} already exists", _0)]
//     ProposalAlreadyExists(SignedProposal<B>),
//     #[display(fmt = "try to insert a different signed_proposal: {}, while exist {}", _0, _1)]
//     InsertDiffProposal(SignedProposal<B>, SignedProposal<B>),
//
//     #[display(fmt = "Other error: {}", _0)]
//     Other(String),
// }
//
// impl<B: Blk> Error for CollectionError<B> {}