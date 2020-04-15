use crate::state::Stage;
use crate::TimeConfig;

pub enum SMREvent {
    NewHeight(Stage, TimeConfig),
    NewRound(Stage),
    PreVote(Stage),
    PreCommit(Stage),
    Brake(Stage),
}

pub enum TimerEvent {
    ProposeTimeout(Stage),
    PreVoteTimeout(Stage),
    PreCommitTimeout(Stage),
    BrakeTimeout(Stage),
    HeightTimeout(Stage),
}
