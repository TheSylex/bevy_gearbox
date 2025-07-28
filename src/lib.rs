use bevy::{platform::collections::HashSet, prelude::*};
use std::marker::PhantomData;

/// Defines a transition between two states in the state machine.
pub struct Connection {
    /// The target state entity to transition to.
    pub target: Entity,
    /// An optional entity holding `Guards` that must be satisfied for this transition to occur.
    pub guards: Option<Entity>,
}

/// An event that triggers a state transition in a machine.
/// This is typically sent by a `TransitionListener` or `ComplexTransitionListener`.
#[derive(Event)]
pub struct Transition {
    /// The state that triggered this transition. This is used to determine the scope
    /// of the transition, especially in parallel state machines.
    pub source: Entity,
    /// The details of the connection, including the target state and any guards.
    pub connection: Connection,
}

/// A component that listens for a specific event `E` and triggers a `Transition`
/// when the event occurs on this entity.
#[derive(Component)]
pub struct TransitionListener<E: Event> {
    connection: Connection,
    _marker: PhantomData<E>,
}

impl<E: Event> TransitionListener<E> {
    pub fn new(connection: Connection) -> Self {
        Self {
            connection,
            _marker: PhantomData,
        }
    }
}

/// A system that handles events for entities with a `TransitionListener`.
/// When an event `E` is triggered on an entity with a `TransitionListener<E>`,
/// this system fires a `Transition` event targeting the state machine's root.
pub fn transition_listener<E: Event>(
    trigger: Trigger<E>,
    listener_query: Query<&TransitionListener<E>>,
    child_of_query: Query<&ChildOf>,
    mut commands: Commands,
) {
    let target = trigger.target();
    let Ok(listener) = listener_query.get(target) else {
        return;
    };

    let root_entity = child_of_query.root_ancestor(target);

    commands.trigger_targets(
        Transition {
            source: target,
            connection: Connection {
                target: listener.connection.target,
                guards: listener.connection.guards,
            },
        },
        root_entity,
    );
}

/// A trait for more complex transition logic where the target state or guards
/// depend on the content of the triggering event.
pub trait ComplexTransitionListener: Component {
    /// The type of event this listener reacts to.
    type Event;

    /// A method to dynamically determine the `Connection` based on the event data.
    fn get_connection(&self, event: &Self::Event) -> Connection;
}

/// A system that handles events for entities with a `ComplexTransitionListener`.
/// This allows for dynamic transitions where the target state is determined by the
/// event's data.
pub fn complex_transition_listener<T: ComplexTransitionListener>(
    trigger: Trigger<T::Event>,
    listener_query: Query<&T>,
    child_of_query: Query<&ChildOf>,
    mut commands: Commands,
) {
    let target = trigger.target();
    let Ok(listener) = listener_query.get(target) else {
        return;
    };

    let root_entity = child_of_query.root_ancestor(target);

    let connection = listener.get_connection(&trigger.event());

    commands.trigger_targets(
        Transition {
            source: target,
            connection: Connection {
                target: connection.target,
                guards: connection.guards,
            },
        },
        root_entity,
    );
}

/// A marker component for a state that has parallel (orthogonal) regions.
/// When a state with this component is entered, the machine will simultaneously enter
/// the initial state of each of its direct children.
#[derive(Component)]
pub struct Parallel;

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

/// A component that specifies the initial substate for a state.
/// When a state is entered, the machine will recursively drill down through `InitialState`
/// components to find the leaf state(s) to activate.
#[derive(Component)]
pub struct InitialState(pub Entity);

/// A component on the state machine's root entity that tracks the current active
/// leaf states. In a machine with parallel regions, this can contain multiple entities.
#[derive(Component)]
pub struct CurrentState(pub HashSet<Entity>);

/// An event that is triggered on a state entity when it is being entered.
#[derive(Event)]
pub struct EnterState;

/// An event that is triggered on a state entity when it is being exited.
#[derive(Event)]
pub struct ExitState;

/// The core system that observes `Transition` events and orchestrates the state change.
/// It calculates the exit and entry paths, sends `ExitState` and `EnterState` events
/// to the appropriate states, and updates the machine's `CurrentState`.
/// Also handles history state saving and restoration.
pub fn transition_observer(
    trigger: Trigger<Transition>,
    mut machine_query: Query<&mut CurrentState>,
    guards_query: Query<&Guards>,
    parallel_query: Query<&Parallel>,
    children_query: Query<&Children>,
    initial_state_query: Query<&InitialState>,
    history_query: Query<&History>,
    mut history_state_query: Query<&mut HistoryState>,
    child_of_query: Query<&ChildOf>,
    mut commands: Commands,
) {
    let machine_entity = trigger.target();
    let source_state = trigger.event().source;
    let new_super_state = trigger.event().connection.target;
    let guards_entity = trigger.event().connection.guards;

    // If the transition is protected by a guard check the guard
    if let Some(guard_entity) = guards_entity {
        if let Ok(guards) = guards_query.get(guard_entity) {
            if !guards.check() {
                return;
            }
        }
    }

    let Ok(mut current_state) = machine_query.get_mut(machine_entity) else {
        return;
    };

    // Handle initialization case where there are no current active states
    if current_state.0.is_empty() {
        
        // Directly enter the target state and drill down to leaf states
        commands.trigger_targets(EnterState, new_super_state);
        
        let new_leaf_states = get_all_leaf_states(
            new_super_state,
            &initial_state_query,
            &children_query,
            &parallel_query,
            &history_query,
            &history_state_query,
            &child_of_query,
            &mut commands,
        );
        current_state.0.extend(new_leaf_states);
        return;
    }

    // Find which of the current leaf states is the one we're transitioning from.
    // It must be a descendant of the state that triggered the transition.
    let Some(exiting_leaf_state) = current_state.0.iter().find(|leaf| {
        **leaf == source_state
            || child_of_query
                .iter_ancestors(**leaf)
                .any(|ancestor| ancestor == source_state)
    }).copied() else {
        // This transition is not coming from any of the currently active states.
        return;
    };

    // Collect exit path from current leaf state up to the root
    let exit_path = get_path_to_root(exiting_leaf_state, &child_of_query);

    // Collect enter path from new super state up to the root
    let enter_path = get_path_to_root(new_super_state, &child_of_query);

    // Find how many ancestors are shared from the root.
    let lca_depth = exit_path
        .iter()
        .rev()
        .zip(enter_path.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();

    // The states to exit are those not in the common path.
    let states_to_exit = &exit_path[..exit_path.len() - lca_depth];

    // The states to enter are those not in the common path.
    let states_to_enter = &enter_path[..enter_path.len() - lca_depth];

    // Exit from child to parent, saving history if needed
    for entity in states_to_exit {
        // Save history if this state has history behavior
        if let Ok(history) = history_query.get(*entity) {
            let states_to_save = match history {
                History::Shallow => {
                    // For shallow history, only save direct children that are currently active
                    current_state.0.iter()
                        .filter(|&&state| {
                            if let Ok(parent) = child_of_query.get(state).map(|child_of| child_of.parent()) {
                                parent == *entity
                            } else {
                                false
                            }
                        })
                        .copied()
                        .collect()
                }
                History::Deep => {
                    // For deep history, save all descendant states that are currently active
                    current_state.0.iter()
                        .filter(|&&state| {
                            state == *entity || child_of_query
                                .iter_ancestors(state)
                                .any(|ancestor| ancestor == *entity)
                        })
                        .copied()
                        .collect()
                }
            };
            
            // Insert or update the history state
            if let Ok(mut existing_history) = history_state_query.get_mut(*entity) {
                existing_history.0 = states_to_save;
            } else {
                commands.entity(*entity).insert(HistoryState(states_to_save));
            }
        }
        
        commands.trigger_targets(ExitState, *entity);
    }

    // Update the current state set
    current_state.0.remove(&exiting_leaf_state);

    // Enter from parent to child
    for entity in states_to_enter.iter().rev() {
        commands.trigger_targets(EnterState, *entity);
    }

    // Now, from the entered super state, drill down to the new leaf states.
    let new_leaf_states = get_all_leaf_states(
        new_super_state,
        &initial_state_query,
        &children_query,
        &parallel_query,
        &history_query,
        &history_state_query,
        &child_of_query,
        &mut commands,
    );
    current_state.0.extend(new_leaf_states);
}

fn get_path_to_root(start_entity: Entity, child_of_query: &Query<&ChildOf>) -> Vec<Entity> {
    let mut path = vec![start_entity];
    path.extend(child_of_query.iter_ancestors(start_entity));
    path
}

pub fn get_all_leaf_states(
    start_node: Entity,
    initial_state_query: &Query<&InitialState>,
    children_query: &Query<&Children>,
    parallel_query: &Query<&Parallel>,
    history_query: &Query<&History>,
    history_state_query: &Query<&mut HistoryState>,
    child_of_query: &Query<&ChildOf>,
    commands: &mut Commands,
) -> HashSet<Entity> {
    let mut leaves = HashSet::new();
    let mut stack = vec![start_node];

    while let Some(entity) = stack.pop() {
        let mut found_next = false;

        // If it's a parallel state, explore all children regions.
        if parallel_query.get(entity).is_ok() {
            if let Ok(children) = children_query.get(entity) {
                found_next = true;
                for child in children {
                    // Enter the state for the region itself
                    commands.trigger_targets(EnterState, *child);
                    stack.push(*child);
                }
            }
        }
        // Check for history first, then fall back to initial state
        else if let (Ok(history), Ok(history_state)) = (history_query.get(entity), history_state_query.get(entity)) {
            found_next = true;
            
            match history {
                History::Shallow => {
                    // For shallow history, restore direct children and let them drill down normally
                    for &saved_state in &history_state.0 {
                        commands.trigger_targets(EnterState, saved_state);
                        stack.push(saved_state);
                    }
                }
                History::Deep => {
                    // For deep history, restore the exact hierarchy that was saved
                    for &saved_state in &history_state.0 {
                        if saved_state != entity {
                            commands.trigger_targets(EnterState, saved_state);
                        }
                        leaves.insert(saved_state);
                    }
                    // Skip normal processing for deep history
                    continue;
                }
            }
        }
        // If it has a single initial state, explore that.
        else if let Ok(initial_state) = initial_state_query.get(entity) {
            found_next = true;

            // To enter a deeply nested initial state, we must first enter all of its parents
            // that are descendants of the current state (`entity`).
            let mut path_to_substate = vec![initial_state.0];
            path_to_substate.extend(
                child_of_query
                    .iter_ancestors(initial_state.0)
                    .take_while(|&ancestor| ancestor != entity),
            );

            // Enter from parent to child
            for e in path_to_substate.iter().rev() {
                commands.trigger_targets(EnterState, *e);
            }

            stack.push(initial_state.0);
        }

        // Otherwise, it's a leaf state.
        if !found_next {
            leaves.insert(entity);
        }
    }
    leaves
}

/// A component that when added to a state entity, will insert the contained component
/// `T` into the state machine's root entity when this state is entered.
#[derive(Component)]
pub struct InsertRootWhileActive<T: Component>(pub T);

/// A component that when added to a state entity, will remove the component type `T`
/// from the state machine's root entity when this state is entered, and restore
/// the stored value when the state is exited.
#[derive(Component)]
pub struct RemoveRootWhileActive<T: Component + Clone>(pub T);

/// A generic system that adds a component `T` to the state machine's root entity
/// when a state with `InsertRootWhileActive<T>` is entered.
pub fn insert_root_while_enter<T: Component + Clone>(
    trigger: Trigger<EnterState>,
    query: Query<&InsertRootWhileActive<T>>,
    child_of_query: Query<&ChildOf>,
    mut commands: Commands,
) {
    let entered_state = trigger.target();
    let Ok(insert_component) = query.get(entered_state) else {
        return;
    };

    let root_entity = child_of_query.root_ancestor(entered_state);

    if root_entity != entered_state {
        commands.entity(root_entity).insert(insert_component.0.clone());
    }
}

/// A generic system that removes a component `T` from the state machine's root entity
/// when a state with `InsertRootWhileActive<T>` is exited.
pub fn insert_root_while_exit<T: Component>(
    trigger: Trigger<ExitState>,
    query: Query<&InsertRootWhileActive<T>>,
    child_of_query: Query<&ChildOf>,
    mut commands: Commands,
) {
    let exited_state = trigger.target();
    if !query.contains(exited_state) {
        return;
    };

    let root_entity = child_of_query.root_ancestor(exited_state);

    if root_entity != exited_state {
        commands.entity(root_entity).remove::<T>();
    }
}

/// A generic system that removes a component `T` from the state machine's root entity
/// when a state with `RemoveRootWhileActive<T>` is entered.
pub fn remove_root_while_enter<T: Component + Clone>(
    trigger: Trigger<EnterState>,
    query: Query<&RemoveRootWhileActive<T>>,
    child_of_query: Query<&ChildOf>,
    mut commands: Commands,
) {
    let entered_state = trigger.target();
    if !query.contains(entered_state) {
        return;
    };

    let root_entity = child_of_query.root_ancestor(entered_state);

    if root_entity != entered_state {
        commands.entity(root_entity).remove::<T>();
    }
}

/// A generic system that restores a component `T` to the state machine's root entity
/// when a state with `RemoveRootWhileActive<T>` is exited, using the stored clone.
pub fn remove_root_while_exit<T: Component + Clone>(
    trigger: Trigger<ExitState>,
    query: Query<&RemoveRootWhileActive<T>>,
    child_of_query: Query<&ChildOf>,
    mut commands: Commands,
) {
    let exited_state = trigger.target();
    let Ok(remove_component) = query.get(exited_state) else {
        return;
    };

    let root_entity = child_of_query.root_ancestor(exited_state);

    if root_entity != exited_state {
        commands.entity(root_entity).insert(remove_component.0.clone());
    }
}

/// A component that holds a set of conditions that must be met for a transition to occur.
#[derive(Component)]
pub struct Guards {
    /// A set of string identifiers for the guards. For a transition to be allowed,
    /// this set must be empty.
    pub guards: HashSet<String>,
}

impl Guards {
    /// Creates a new, empty set of guards.
    pub fn new() -> Self {
        Self {
            guards: HashSet::new(),
        }
    }

    /// Adds a guard to the set. The guard is identified by its name.
    pub fn add_guard(&mut self, guard: impl Guard) {
        self.guards.insert(guard.name());
    }

    /// Removes a guard from the set.
    pub fn remove_guard(&mut self, guard: impl Guard) {
        self.guards.remove(&guard.name());
    }

    /// Checks if the guard conditions are met. Currently, this just checks if the set is empty.
    pub fn check(&self) -> bool {
        self.guards.is_empty()
    }
}

/// A trait for components that act as a guard. Guards are components that can be
/// added or removed from a `Guards` entity to dynamically enable or disable transitions.
pub trait Guard: Component {
    /// Returns the unique string identifier for this guard type.
    fn name(&self) -> String;
}

#[derive(Component)]
pub struct Active;

#[derive(Component)]
pub struct Inactive;

fn add_active(
    trigger: Trigger<EnterState>,
    mut commands: Commands,
) {
    let target = trigger.target();
    commands.entity(target).remove::<Inactive>().insert(Active);
}

fn add_inactive(
    trigger: Trigger<ExitState>,
    mut commands: Commands,
) {
    let target = trigger.target();
    commands.entity(target).remove::<Active>().insert(Inactive);
}

pub fn propagate_event<T: Event + Clone>(
    trigger: Trigger<T>,
    query: Query<&CurrentState>,
    mut commands: Commands,
) {
    let target = trigger.target();
    let Ok(current_state) = query.get(target) else {
        return;
    };

    for state in current_state.0.iter() {
        commands.trigger_targets(trigger.event().clone(), *state);
    }
}

/// An event fired when the state machine should be initialized to its initial state.
#[derive(Event, Clone)]
pub struct InitializeMachine;

/// Triggers the InitializeMachine event when AbilityMachine component is added.
fn initialize_state_machine(
    trigger: Trigger<OnAdd, TransitionListener::<InitializeMachine>>,
    mut commands: Commands,
) {
    let target = trigger.target();
    commands.trigger_targets(InitializeMachine, target);
}

/// A prelude for easily importing the most common types from the library.
pub mod prelude {
    pub use crate::{
        // Structs
        Connection,
        // Events
        EnterState,
        ExitState,
        Transition,
        // Components
        Active,
        CurrentState,
        Guards,
        HistoryState,
        InitialState,
        InitializeMachine,
        Inactive,
        InsertRootWhileActive,
        Parallel,
        RemoveRootWhileActive,
        TransitionListener,
        // Enums
        History,
        // Traits
        ComplexTransitionListener,
        Guard,
        // Systems
        complex_transition_listener,
        get_all_leaf_states,
        insert_root_while_enter,
        insert_root_while_exit,
        propagate_event,
        remove_root_while_enter,
        remove_root_while_exit,
        transition_listener,
    };
}

/// The main plugin for `bevy_gearbox`. Registers events and adds the core systems.
pub struct GearboxPlugin;

impl Plugin for GearboxPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(transition_observer)
            .add_observer(add_active)
            .add_observer(add_inactive)
            .add_observer(initialize_state_machine)
            .add_observer(transition_listener::<InitializeMachine>);
    }
}