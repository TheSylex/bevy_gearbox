use bevy::platform::collections::HashSet;

// StateAggregator trait for components that track multiple blocking conditions
pub trait StateAggregator {
    fn add_blocker(&mut self, blocker: &str);
    fn remove_blocker(&mut self, blocker: &str);
    fn is_blocked(&self) -> bool;
    fn is_blocked_by(&self, blocker: &str) -> bool;
    fn get_blockers(&self) -> &HashSet<String>;
}