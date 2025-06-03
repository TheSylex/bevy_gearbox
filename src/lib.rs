use bevy::{hierarchy::HierarchyQueryExt, prelude::*};

pub mod commands;
pub mod components;
pub mod iter;
pub mod prelude;
pub mod state_aggregator;

// re-export state_machine macro
pub use macros::state_machine;

// re-export essential state machine functionality
pub use commands::{OnEnterState, OnExitState, StateTransitionCommandsExt};
pub use components::{RestingState, WorkingState, InChildSMState, FinishedChildSMState, FizzledState};
pub use iter::HierarchyQueryExt as GearboxHierarchyQueryExt;
pub use state_aggregator::StateAggregator;

/// HSM impl using triggers and observers.
/// 
/// When a character activates an ability they will be put into InChildSMState(ability_entity).
/// The ability_entity will transition from IdleState to WorkingState. Eventually the 
/// ability_entity will transition to IdleState. At this point, the parent transitions
/// back to IdleState.
/// 
/// Sub state machines usually exist to change the state of their top-most parent SM entity. 
/// In other words, the bottom-most active state machine dictates the behavior of the top most entity.
/// Imagine an ability as a sub-SM a character can enter. Now imagine that ability has its own 
/// sub-SMs. You can make arbitrarily deep heirarchies, and the bottom level will dictate the 
/// behavior of the top. 

/// If a SM is set to InChildSMState, we need to update the child SM state to
/// WorkingState.
fn set_child_working_system(
    trigger: Trigger<OnEnterState<InChildSMState>>,
    child_sm_query: Query<&RestingState>,
    mut commands: Commands,
) {
    let parent_sm_entity = trigger.entity();
    let child_sm_entity = trigger.event().0.0;

    if let Ok(resting_state) = child_sm_query.get(child_sm_entity) {
        if resting_state.is_blocked() {
            // Child SM is blocked (e.g., requirements not met), reset parent to RestingState
            commands.entity(parent_sm_entity).transition(RestingState::new());
        } else {
            // Child SM is ready, proceed to WorkingState
            commands.entity(child_sm_entity).transition(WorkingState);
        }
    } else {
        // Child SM doesn't have RestingState, reset parent to RestingState
        commands.entity(parent_sm_entity).transition(RestingState::new());
    }
}

/// Triggers in cases where the parent leaves the child state early, causing the child
/// state to enter a special "fizzle" state. This could happen if a character were invoking
/// an ability and were suddenly stunned, interupting the ability substate.
fn early_exit_child_state_trigger_system(
    trigger: Trigger<OnExitState<InChildSMState>>,
    parent_sm_query: Query<&InChildSMState>,
    child_sm_query: Query<Entity, Without<RestingState>>,
    mut commands: Commands,
) {
    let parent_sm_entity = trigger.entity();

    let Ok(in_child_sm) = parent_sm_query.get(parent_sm_entity) else {
        return;
    };

    let Ok(child_sm_entity) = child_sm_query.get(in_child_sm.0) else {
        return;
    };

    commands.entity(child_sm_entity).transition(FizzledState);
}

/// Triggers when an entity enters the RestingState. Finds the closest ancestor that has
/// InChildSMState pointing to this entity and transitions it to FinishedChildSMState.
fn return_to_parent_sm_system(
    trigger: Trigger<OnEnterState<RestingState>>,
    parent_query: Query<&Parent>,
    child_query: Query<&InChildSMState>,
    mut commands: Commands,
) {
    let child_sm_entity = trigger.entity();

    // Find the closest ancestor that has an InChildSMState pointing to this entity
    for ancestor in parent_query.iter_ancestors(child_sm_entity) {
        if let Ok(in_child_sm_state) = child_query.get(ancestor) {
            if in_child_sm_state.0 == child_sm_entity {
                commands.entity(ancestor).transition(FinishedChildSMState(child_sm_entity));
                return;
            }
        }
    }
}

pub struct GearboxPlugin;

impl Plugin for GearboxPlugin {
    fn build(&self, app: &mut App) {
        app 
            .add_observer(set_child_working_system)
            .add_observer(return_to_parent_sm_system)
            .add_observer(early_exit_child_state_trigger_system);
    }
}