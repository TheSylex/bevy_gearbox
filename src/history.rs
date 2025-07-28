use bevy::{platform::collections::HashSet, prelude::*};

/// A component that enables history behavior for a state.
/// When a state with this component is exited and later re-entered,
/// it will restore previously active substates instead of using InitialState.
/// Defines the type of history behavior for a state.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum History {
    /// Remember only the direct child state that was active when last exited.
    /// On re-entry, restore that direct child and follow normal InitialState logic from there.
    Shallow,
    /// Remember the entire hierarchy of substates that were active when last exited.
    /// On re-entry, restore the exact nested hierarchy that was previously active.
    Deep,
}

/// A component that stores the previously active states for history restoration.
/// This is automatically managed by the history systems.
#[derive(Component)]
pub struct HistoryState(pub HashSet<Entity>);
