use bevy::{platform::collections::HashSet, prelude::*, reflect::Reflect};
use bevy_ecs::component::Mutable;
use bevy_ecs::{component::StorageType, reflect::ReflectMapEntities};
use bevy_ecs::entity::MapEntities;

use crate::{active::{Active, Inactive}, guards::Guards, history::{History, HistoryState}};

pub mod active;
pub mod guards;
pub mod history;
pub mod prelude;
pub mod state_component;
pub mod transition_listener;

/// The main plugin for `bevy_gearbox`. Registers events and adds the core systems.
pub struct GearboxPlugin;

impl Plugin for GearboxPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(transition_observer)
            .add_observer(active::add_active)
            .add_observer(active::add_inactive)
            .add_observer(initialize_state_machine)
            .add_observer(always);

        app.register_type::<StateMachineRoot>();
        app.register_type::<Connection>();
        app.register_type::<Parallel>();
        app.register_type::<InitialState>();
        app.register_type::<CurrentState>();
        app.register_type::<History>();
        app.register_type::<HistoryState>();
        app.register_type::<ChildOf>();
        app.register_type::<Guards>();
        app.register_type::<Active>();
        app.register_type::<Inactive>();
        app.register_type::<EnterState>();
        app.register_type::<ExitState>();
        app.register_type::<OnAdd>();

        app.add_systems(Update, check_always_system);
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct StateMachineRoot;

pub fn find_state_machine_root(
    entity: Entity,
    child_of_query: &Query<&ChildOf>,
    state_machine_root_query: &Query<&StateMachineRoot>,
) -> Option<Entity> {
    for entity in child_of_query.iter_ancestors(entity) {
        if state_machine_root_query.get(entity).is_ok() {
            return Some(entity);
        }
    }
    None
}

#[derive(Reflect, Clone, Debug)]
#[reflect(MapEntities)]
pub struct Connection {
    /// The target state entity to transition to.
    pub target: Entity,
    /// An optional entity holding `Guards` that must be satisfied for this transition to occur.
    pub guards: Option<Entity>,
}

impl MapEntities for Connection {
    fn map_entities<E: EntityMapper>(&mut self, entity_mapper: &mut E) {
        self.target = entity_mapper.get_mapped(self.target);
        if let Some(guards) = self.guards {
            self.guards = Some(entity_mapper.get_mapped(guards));
        }
    }
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

/// A marker component for a state that has parallel (orthogonal) regions.
/// When a state with this component is entered, the machine will simultaneously enter
/// the initial state of each of its direct children.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct Parallel;

/// A component that specifies the initial substate for a state.
/// When a state is entered, the machine will recursively drill down through `InitialState`
/// components to find the leaf state(s) to activate.
#[derive(Reflect)]
#[reflect(Component)]
pub struct InitialState(pub Entity);

impl Component for InitialState {
    const STORAGE_TYPE: StorageType = StorageType::Table;

    type Mutability = Mutable;
    
    fn map_entities<E: EntityMapper>(this: &mut Self, entity_mapper: &mut E) {
        this.0 = entity_mapper.get_mapped(this.0);
    }
}

/// A component on the state machine's root entity that tracks the current active
/// leaf states. In a machine with parallel regions, this can contain multiple entities.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct CurrentState(pub HashSet<Entity>);

/// An event that is triggered on a state entity when it is being entered.
#[derive(Event, Reflect, Default)]
pub struct EnterState;

/// An event that is triggered on a state entity when it is being exited.
#[derive(Event, Reflect, Default)]
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

/// Triggers the InitializeMachine event when AbilityMachine component is added.
fn initialize_state_machine(
    trigger: Trigger<OnAdd, StateMachineRoot>,
    initial_state_query: Query<&InitialState>,
    mut commands: Commands,
) {
    let target = trigger.target();
    let Ok(initial_state) = initial_state_query.get(target) else {
        return;
    };

    commands.trigger_targets(
        Transition {
            source: target,
            connection: Connection {
                target: initial_state.0,
                guards: None,
            },
        },
        target,
    );
}

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct Always {
    pub target: Entity,
    pub guards: Option<Entity>,
}

fn always(
    trigger: Trigger<EnterState>,
    query: Query<&Always>,
    child_of_query: Query<&ChildOf>,
    state_machine_root_query: Query<&StateMachineRoot>,
    mut commands: Commands,
) {
    let target = trigger.target();
    let Ok(always) = query.get(target) else {
        return;
    };
    
    let root_entity = find_state_machine_root(target, &child_of_query, &state_machine_root_query);

    let Some(root_entity) = root_entity else {
        return;
    };

    commands.trigger_targets(Transition {
        source: target,
        connection: Connection {
            target: always.target,
            guards: always.guards,
        },
    }, root_entity);
}

fn check_always_system(
    query: Query<(Entity, &Always, &Guards), With<Active>>,
    child_of_query: Query<&ChildOf>,
    state_machine_root_query: Query<&StateMachineRoot>,
    mut commands: Commands,
) {
    for (entity, always, guards) in query.iter() {
        if guards.check() {
            let root_entity = find_state_machine_root(entity, &child_of_query, &state_machine_root_query);
        
            let Some(root_entity) = root_entity else {
                return;
            };

            commands.trigger_targets(Transition {
                source: entity,
                connection: Connection {
                    target: always.target,
                    guards: always.guards,
                },
            }, root_entity);
        }
    }
}