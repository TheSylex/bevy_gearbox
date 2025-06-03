use bevy::prelude::*;
use bevy_gearbox::{prelude::{FinishedChildSMState, InChildSMState, RestingState, StateTransitionCommandsExt, WorkingState}, state_aggregator::StateAggregator, *};
use bevy_gearbox_macros::state_machine;

#[derive(Component, Reflect)]
struct TestCharacter;

// Define states for testing
#[derive(Component, Clone, Debug, Default)]
struct IdleState;

#[derive(Component, Clone, Debug)]
struct FizzledState;

// Use the macro to create a test state machine
state_machine!(TestCharacter; 
    IdleState,
    RestingState,
    WorkingState,
    InChildSMState, 
    FinishedChildSMState,
    FizzledState,
);

#[derive(Component, Clone, Debug)]
struct Ability;

state_machine!(Ability;
    RestingState,
    WorkingState,
    InChildSMState, 
    FinishedChildSMState,
    FizzledState,
);

#[test]
fn test_resting_state_blocking() {
    let mut resting_state = RestingState::new();
    assert!(!resting_state.is_blocked());
    
    resting_state.add_blocker("stunned");
    assert!(resting_state.is_blocked());
    assert!(resting_state.is_blocked_by("stunned"));
    
    resting_state.remove_blocker("stunned");
    assert!(!resting_state.is_blocked());
}

#[test]
fn test_basic_state_transition() {
    let mut app = App::new();
    app.add_plugins(GearboxPlugin);
    app.add_plugins(TestCharacterPlugin);
    app.add_plugins(AbilityPlugin);

    let parent_entity = app.world_mut().spawn((
        TestCharacter, 
        IdleState,
    )).id();
    let child_entity = app.world_mut().spawn(Ability)
    .set_parent(parent_entity)
    .id();

    // Transition parent to child state
    app.world_mut().commands().entity(parent_entity).transition(InChildSMState(child_entity));
    app.update();

    // Check that child transitioned to WorkingState
    assert!(app.world().entity(child_entity).contains::<WorkingState>());
}

#[test]
fn test_macro_generated_state_transition() {
    let mut app = App::new();
    app.add_plugins(GearboxPlugin);
    app.add_plugins(TestCharacterPlugin);

    let entity = app.world_mut().spawn(TestCharacter).id();
    app.update(); // Let the enum get added

    // Transition to WorkingState
    app.world_mut().commands().entity(entity).transition(WorkingState);
    app.update();

    assert!(app.world().entity(entity).contains::<WorkingState>());
    assert!(app.world().entity(entity).contains::<TestCharacterStateEnum>());
}

#[test]
fn test_blocked_child_state() {
    let mut app = App::new();
    app.add_plugins(GearboxPlugin);
    app.add_plugins(TestCharacterPlugin);

    let parent_entity = app.world_mut().spawn(TestCharacter).id();
    
    let mut blocked_resting_state = RestingState::new();
    blocked_resting_state.add_blocker("stunned");
    let child_entity = app.world_mut().spawn(blocked_resting_state).id();

    // Set up parent-child relationship
    app.world_mut().entity_mut(child_entity).set_parent(parent_entity);

    // Transition parent to child state
    app.world_mut().commands().entity(parent_entity).transition(InChildSMState(child_entity));
    app.update();

    // Check that parent went back to RestingState because child was blocked
    assert!(app.world().entity(parent_entity).contains::<RestingState>());
    assert!(!app.world().entity(parent_entity).contains::<InChildSMState>());
}

#[test]
fn test_child_completion_flow() {
    let mut app = App::new();
    app.add_plugins(GearboxPlugin);
    app.add_plugins(TestCharacterPlugin);
    app.add_plugins(AbilityPlugin);

    let parent_entity = app.world_mut().spawn(TestCharacter).id();
    let child_entity = app.world_mut().spawn(Ability)
    .set_parent(parent_entity)
    .id();

    // Transition parent to child state
    app.world_mut().commands().entity(parent_entity).transition(InChildSMState(child_entity));
    app.update();

    // Child should be in WorkingState
    assert!(app.world().entity(child_entity).contains::<WorkingState>());

    // Complete the child by returning to RestingState
    app.world_mut().commands().entity(child_entity).transition(RestingState::new());
    app.update();

    // Parent should now be in FinishedChildSMState
    assert!(app.world().entity(parent_entity).contains::<FinishedChildSMState>());
} 