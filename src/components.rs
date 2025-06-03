use bevy::{prelude::*, utils::HashSet};
use crate::state_aggregator::StateAggregator;

/// Represents a state machine that is ready to be entered, or that has "finished" and needs to
/// pass control back up the hierarchy.
#[derive(Component, Clone, Debug, Default)]
pub struct RestingState {
    blockers: HashSet<String>,
}

impl RestingState {
    pub fn new() -> Self {
        Self {blockers: HashSet::new()}
    }
}

impl StateAggregator for RestingState {
    fn add_blocker(&mut self, blocker: &str) {
        self.blockers.insert(blocker.to_string());
    }

    fn remove_blocker(&mut self, blocker: &str) {
        self.blockers.remove(blocker);
    }

    fn is_blocked(&self) -> bool {
        !self.blockers.is_empty()
    }

    fn is_blocked_by(&self, blocker: &str) -> bool {
        self.blockers.contains(blocker)
    }

    fn get_blockers(&self) -> &HashSet<String> {
        &self.blockers
    }
}

/// The first state a child SM enters when activated by its parent.
#[derive(Component, Clone, Debug)]
pub struct WorkingState;

/// Represents a state machine that has failed to finish.
#[derive(Component, Clone, Debug)]
pub struct FizzledState;

/// Added when a state machine drops into another state. Denotes the sub state a SM is in.
#[derive(Component, Clone, Debug)]
pub struct InChildSMState(pub Entity);

#[derive(Component, Clone, Debug)]
pub struct FinishedChildSMState(pub Entity);