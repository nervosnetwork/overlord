use crate::state::Stage;

pub enum SMREvent {
    NewHeight(Stage),
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
