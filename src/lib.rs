use bevy::{platform::collections::HashSet, prelude::*, reflect::Reflect};
use bevy_ecs::component::Mutable;
use bevy_ecs::{component::StorageType};

use crate::{active::{Active, Inactive}, guards::Guards, history::{History, HistoryState}};

pub mod active;
pub mod guards;
pub mod history;
pub mod prelude;
pub mod state_component;
pub mod transitions;

/// The main plugin for `bevy_gearbox`. Registers events and adds the core systems.
pub struct GearboxPlugin;

impl Plugin for GearboxPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(transition_observer)
            .add_observer(active::add_active)
            .add_observer(active::add_inactive)
            .add_observer(initialize_state_machine);

        app.register_type::<Parallel>();
        app.register_type::<InitialState>();
        app.register_type::<StateMachine>();
        app.register_type::<History>();
        app.register_type::<HistoryState>();
        app.register_type::<StateChildren>();
        app.register_type::<StateChildOf>();
        app.register_type::<Guards>();
        app.register_type::<Active>();
        app.register_type::<Inactive>();
        app.register_type::<EnterState>();
        app.register_type::<ExitState>();
        app.register_type::<TransitionActions>();
        app.register_type::<OnAdd>();
        app.register_type::<transitions::Source>();
        app.register_type::<transitions::Transitions>();
        app.register_type::<transitions::Target>();
        app.register_type::<transitions::AlwaysEdge>();
        app.register_type::<transitions::TransitionKind>();

        app.add_observer(transitions::transition_always);
        app.add_observer(transitions::start_after_on_enter);
        app.add_observer(transitions::cancel_after_on_exit);
        app.add_systems(Update, transitions::check_always_on_guards_changed);
        app.add_systems(Update, transitions::tick_after_system);
    }
}

// State-specific hierarchy relationships
#[derive(Component, Default, Debug, PartialEq, Eq, Reflect)]
#[relationship_target(relationship = StateChildOf, linked_spawn)]
#[reflect(Component, FromWorld, Default)]
pub struct StateChildren(Vec<Entity>);

impl<'a> IntoIterator for &'a StateChildren {
    type Item = <Self::IntoIter as Iterator>::Item;

    type IntoIter = std::slice::Iter<'a, Entity>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl StateChildren {
    pub fn new() -> Self {
        Self(Vec::new())
    }
}

#[derive(Component, Clone, PartialEq, Eq, Debug, Reflect)]
#[relationship(relationship_target = StateChildren)]
#[reflect(Component, PartialEq, Debug, FromWorld, Clone)]
pub struct StateChildOf(#[entities] pub Entity);

impl FromWorld for StateChildOf {
    #[inline(always)]
    fn from_world(_world: &mut World) -> Self {
        StateChildOf(Entity::PLACEHOLDER)
    }
}

/// An event that triggers a state transition in a machine.
/// Prefer using `edge` to reference a transition entity. The legacy `connection`
/// field is retained for initialization and backward compatibility.
#[derive(Event)]
pub struct Transition {
    /// The state that triggered this transition. This is used to determine the scope
    /// of the transition, especially in parallel state machines.
    pub source: Entity,
    /// The transition edge entity that defines the target and kind.
    pub edge: Entity,
}

#[derive(Event, Reflect)]
pub struct TransitionActions {
    pub source: Entity,
    pub edge: Entity,
    pub target: Entity,
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
pub struct StateMachine(pub HashSet<Entity>);

impl StateMachine {
    pub fn new() -> Self {
        Self(HashSet::new())
    }
}

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
    mut machine_query: Query<&mut StateMachine>,
    parallel_query: Query<&Parallel>,
    children_query: Query<&StateChildren>,
    initial_state_query: Query<&InitialState>,
    history_query: Query<&History>,
    mut history_state_query: Query<&mut HistoryState>,
    child_of_query: Query<&StateChildOf>,
    edge_target_query: Query<&transitions::Target>,
    kind_query: Query<&transitions::TransitionKind>,
    mut commands: Commands,
) {
    let machine_entity = trigger.target();
    let source_state = trigger.event().source;
    // Resolve target: prefer Target on the edge; otherwise treat the edge itself
    // as the super state to start from (useful for root init where initial is on the state).
    let new_super_state = match edge_target_query.get(trigger.event().edge) {
        Ok(edge_target) => edge_target.0,
        Err(_) => trigger.event().edge,
    };

    let Ok(mut current_state) = machine_query.get_mut(machine_entity) else {
        return;
    };

    // Handle initialization case where there are no current active states
    if current_state.0.is_empty() {
        // Enter the machine root first, then all ancestors from root→target
        commands.trigger_targets(EnterState, machine_entity);

        // Build path from target up to (but excluding) the machine root
        let mut path_to_target: Vec<Entity> = vec![new_super_state];
        path_to_target.extend(
            child_of_query
                .iter_ancestors(new_super_state)
                .take_while(|&ancestor| ancestor != machine_entity),
        );

        // Enter ancestors parent→child down to the target
        for entity in path_to_target.iter().rev() {
            commands.trigger_targets(EnterState, *entity);
        }

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

    // Determine whether the source is a parallel parent (transition defined on a parallel state)
    let source_is_parallel = parallel_query.get(source_state).is_ok();

    // Exit/enter computation diverges for parallel parents
    let (states_to_exit_vec, states_to_enter_vec) = if source_is_parallel {
        // 1) Exit: all active leaves under the parallel source up to (and including) the source
        let mut ordered_exits: Vec<Entity> = Vec::new();
        let mut seen: HashSet<Entity> = HashSet::new();
        for &leaf in current_state.0.iter() {
            // Consider only leaves that are descendants of the parallel source
            let is_descendant = leaf == source_state
                || child_of_query.iter_ancestors(leaf).any(|a| a == source_state);
            if !is_descendant { continue; }

            // Exit path from leaf up to source_state (inclusive)
            let path = get_path_to_root(leaf, &child_of_query);
            if let Some(pos) = path.iter().position(|&e| e == source_state) {
                let slice = &path[..=pos]; // includes source_state
                for &e in slice {
                    if seen.insert(e) { ordered_exits.push(e); }
                }
            }
        }

        // 2) Enter: compute LCA between source_state and new_super_state
        let exit_path_from_source = get_path_to_root(source_state, &child_of_query);
        let enter_path = get_path_to_root(new_super_state, &child_of_query);

        let mut lca_depth = exit_path_from_source
            .iter()
            .rev()
            .zip(enter_path.iter().rev())
            .take_while(|(a, b)| a == b)
            .count();

        let lca_entity = if lca_depth > 0 { Some(exit_path_from_source[exit_path_from_source.len() - lca_depth]) } else { None };

        let is_internal = matches!(kind_query.get(trigger.event().edge), Ok(transitions::TransitionKind::Internal));
        if !is_internal {
            // If source is the LCA, default external re-enters the source
            if lca_entity == Some(source_state) {
                lca_depth = lca_depth.saturating_sub(1);
            }
        }

        let states_to_enter = enter_path[..enter_path.len() - lca_depth].to_vec();
        (ordered_exits, states_to_enter)
    } else {
        // Non-parallel: original single-leaf logic
        // Find the leaf that’s under the source
        let Some(exiting_leaf_state) = current_state.0.iter().find(|leaf| {
            **leaf == source_state
                || child_of_query
                    .iter_ancestors(**leaf)
                    .any(|ancestor| ancestor == source_state)
        }).copied() else {
            // This transition is not coming from any of the currently active states.
            return;
        };

        let exit_path = get_path_to_root(exiting_leaf_state, &child_of_query);
        let enter_path = get_path_to_root(new_super_state, &child_of_query);
        let mut lca_depth = exit_path
            .iter()
            .rev()
            .zip(enter_path.iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let lca_entity = if lca_depth > 0 { Some(exit_path[exit_path.len() - lca_depth]) } else { None };
        let is_internal = matches!(kind_query.get(trigger.event().edge), Ok(transitions::TransitionKind::Internal));
        if !is_internal {
            if new_super_state == exiting_leaf_state {
                lca_depth = lca_depth.saturating_sub(1);
            } else if lca_entity == Some(source_state) {
                lca_depth = lca_depth.saturating_sub(1);
            }
        }
        let states_to_exit = exit_path[..exit_path.len() - lca_depth].to_vec();
        let states_to_enter = enter_path[..enter_path.len() - lca_depth].to_vec();
        (states_to_exit, states_to_enter)
    };

    // Exit from child to parent, saving history if needed
    for entity in states_to_exit_vec.iter() {
        // Save history if this state has history behavior
        if let Ok(history) = history_query.get(*entity) {
            let states_to_save = match history {
                History::Shallow => {
                    // For shallow history, only save direct children that are currently active
                    current_state.0.iter()
                        .filter(|&&state| {
                            if let Ok(parent) = child_of_query.get(state).map(|child_of| child_of.0) {
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
    // For parallel parents we potentially exited multiple leaves; remove any leaves we exited.
    for exited in states_to_exit_vec.iter() {
        // Only remove if it was a leaf previously
        current_state.0.remove(exited);
    }

    // Transition actions phase (between exits and entries)
    commands.trigger_targets(
        TransitionActions {
            source: source_state,
            edge: trigger.event().edge,
            target: new_super_state,
        }, 
        machine_entity,
    );

    // Enter from parent to child
    for entity in states_to_enter_vec.iter().rev() {
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

fn get_path_to_root(start_entity: Entity, child_of_query: &Query<&StateChildOf>) -> Vec<Entity> {
    let mut path = vec![start_entity];
    path.extend(child_of_query.iter_ancestors(start_entity));
    path
}

pub fn get_all_leaf_states(
    start_node: Entity,
    initial_state_query: &Query<&InitialState>,
    children_query: &Query<&StateChildren>,
    parallel_query: &Query<&Parallel>,
    history_query: &Query<&History>,
    history_state_query: &Query<&mut HistoryState>,
    child_of_query: &Query<&StateChildOf>,
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
                for &child in children {
                    // Enter the state for the region itself
                    commands.trigger_targets(EnterState, child);
                    stack.push(child);
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
                        // Enter the saved direct child so its entry actions run
                        commands.trigger_targets(EnterState, saved_state);
                        // Then continue drilling via InitialState/History under it
                        stack.push(saved_state);
                    }
                }
                History::Deep => {
                    // For deep history, restore the exact hierarchy that was saved
                    for &saved_state in &history_state.0 {
                        // Compute the path from the current restoring state (entity)
                        // down to the saved leaf and enter along that path in parent→child order.
                        let mut path_to_substate = vec![saved_state];
                        path_to_substate.extend(
                            child_of_query
                                .iter_ancestors(saved_state)
                                .take_while(|&ancestor| ancestor != entity),
                        );

                        for e in path_to_substate.iter().rev() {
                            commands.trigger_targets(EnterState, *e);
                        }

                        // Mark the saved leaf as restored
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

/// Triggers the InitializeMachine event when AbilityMachine component is added.
fn initialize_state_machine(
    trigger: Trigger<OnAdd, StateMachine>,
    initial_state_query: Query<&InitialState>,
    mut commands: Commands,
) {
    let target = trigger.target();
    let Ok(_initial_state) = initial_state_query.get(target) else {
        return;
    };

    // Treat the root as its own edge: transition to the root, then drill down via InitialState/Always.
    commands.trigger_targets(Transition { source: target, edge: target }, target);
}