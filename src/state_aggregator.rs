use bevy::{prelude::*, utils::HashSet};

// StateAggregator trait for components that track multiple blocking conditions
pub trait StateAggregator {
    fn add_blocker(&mut self, blocker: &str);
    fn remove_blocker(&mut self, blocker: &str);
    fn is_blocked(&self) -> bool;
    fn is_blocked_by(&self, blocker: &str) -> bool;
    fn get_blockers(&self) -> &HashSet<String>;
}

// Core state aggregator components
#[derive(Component, Clone, Debug, Default)]
pub struct CanApply {
    pub blockers: HashSet<String>,
}

impl StateAggregator for CanApply {
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

#[derive(Component, Clone, Debug, Default)]
pub struct CanUnapply {
    pub blockers: HashSet<String>,
}

impl StateAggregator for CanUnapply {
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

#[derive(Component, Clone, Debug, Default)]
pub struct StaysActive {
    pub blockers: HashSet<String>,
}

impl StateAggregator for StaysActive {
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