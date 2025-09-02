#![feature(associated_type_defaults)]

use bevy::{prelude::*, reflect::Reflect};
use bevy::platform::collections::HashSet;

use crate::{active::{Active, Inactive}, guards::Guards, history::{History, HistoryState}};

pub mod active;
pub mod guards;
pub mod history;
pub mod prelude;
pub mod state_component;
pub mod transitions;

// Re-export the derive macro and key types for convenience
pub use bevy_gearbox_macros::SimpleTransition;
pub use transitions::{TransitionEvent, NoEvent};

/// The main plugin for `bevy_gearbox`. Registers events and adds the core systems.
pub struct GearboxPlugin;

impl Plugin for GearboxPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(active::add_active)
            .add_observer(active::add_inactive)
            .add_observer(transition_observer::<()>)
            .add_observer(initialize_state_machine)
            .add_observer(reset_state_region)
            .add_observer(transitions::always_edge_listener)
            .add_observer(transitions::start_after_on_enter)
            .add_observer(transitions::cancel_after_on_exit)
            .add_observer(transitions::reset_on_transition_actions);

        app.register_type::<Parallel>()
            .register_type::<InitialState>()
            .register_type::<StateMachine>()
            .register_type::<History>()
            .register_type::<HistoryState>()
            .register_type::<StateChildren>()
            .register_type::<StateChildOf>()
            .register_type::<Guards>()
            .register_type::<Active>()
            .register_type::<Inactive>()
            .register_type::<EnterState>()
            .register_type::<ExitState>()
            .register_type::<ResetRegion>()
            .register_type::<TransitionActions>()
            .register_type::<transitions::After>()
            .register_type::<transitions::Source>()
            .register_type::<transitions::Transitions>()
            .register_type::<transitions::Target>()
            .register_type::<transitions::AlwaysEdge>()
            .register_type::<transitions::EdgeKind>()
            .register_type::<transitions::NoEvent>()
            .register_type::<transitions::ResetEdge>()
            .register_type::<transitions::ResetScope>()
            .register_type::<state_component::Reset>();

        app.add_systems(Update, (
            transitions::check_always_on_guards_changed,
            transitions::tick_after_system,
        ));
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
pub struct Transition<T = ()> where T: Clone + Send + Sync + 'static {
    /// The state that triggered this transition. This is used to determine the scope
    /// of the transition, especially in parallel state machines.
    pub source: Entity,
    /// The transition edge entity that defines the target and kind.
    pub edge: Entity,
    /// Optional typed payload that can be used by the transition observer
    pub payload: T,
}

#[derive(Event, Reflect)]
pub struct TransitionActions;

/// A marker component for a state that has parallel (orthogonal) regions.
/// When a state with this component is entered, the machine will simultaneously enter
/// the initial state of each of its direct children.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct Parallel;

/// A component that specifies the initial substate for a state.
/// When a state is entered, the machine will recursively drill down through `InitialState`
/// components to find the leaf state(s) to activate.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct InitialState(#[entities] pub Entity);

/// A component on the state machine's root entity that tracks the current active states.
/// - `active` contains all active states (root, ancestors, and leaves)
/// - `active_leaves` contains only the active leaf states
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct StateMachine {
    #[entities]
    pub active: HashSet<Entity>,
    #[entities]
    pub active_leaves: HashSet<Entity>,
}

impl StateMachine {
    pub fn new() -> Self {
        Self { active: HashSet::new(), active_leaves: HashSet::new() }
    }

    #[inline]
    pub fn insert(&mut self, entity: Entity) {
        self.active.insert(entity);
    }

    #[inline]
    pub fn insert_leaf(&mut self, entity: Entity) {
        self.active_leaves.insert(entity);
    }

    #[inline]
    pub fn is_active(&self, entity: &Entity) -> bool {
        self.active.contains(entity)
    }

    #[inline]
    pub fn is_leaf_active(&self, entity: &Entity) -> bool {
        self.active_leaves.contains(entity)
    }
}

/// An event that is triggered on a state entity when it is being entered.
#[derive(Event, Reflect, Default)]
pub struct EnterState;

/// An event that is triggered on a state entity when it is being exited.
#[derive(Event, Reflect, Default)]
pub struct ExitState;

/// Event to reset a state machine: clear Active flags under the root and reinitialize
#[derive(Event, Reflect, Default)]
pub struct ResetRegion;

/// The core system that observes `Transition` events and orchestrates the state change.
/// It calculates the exit and entry paths, sends `ExitState` and `EnterState` events
/// to the appropriate states, and updates the machine's `CurrentState`.
/// Also handles history state saving and restoration.
fn transition_observer<T: transitions::PhasePayload>(
    trigger: Trigger<Transition<T>>,
    mut machine_query: Query<&mut StateMachine>,
    parallel_query: Query<&Parallel>,
    children_query: Query<&StateChildren>,
    initial_state_query: Query<&InitialState>,
    history_query: Query<&History>,
    mut history_state_query: Query<&mut HistoryState>,
    child_of_query: Query<&StateChildOf>,
    edge_target_query: Query<&transitions::Target>,
    kind_query: Query<&transitions::EdgeKind>,
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
    if current_state.active_leaves.is_empty() {
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
        current_state.active_leaves.extend(new_leaf_states);
        // Derive full active set from leaves
        current_state.active = compute_active_from_leaves(&current_state.active_leaves, &child_of_query);
        return;
    }

    // Determine whether the source is a parallel parent (transition defined on a parallel state)
    let source_is_parallel = parallel_query.get(source_state).is_ok();

    // Exit/enter computation diverges for parallel parents
    let (states_to_exit_vec, states_to_enter_vec) = if source_is_parallel {
        // 1) Exit: all active leaves under the parallel source up to (and including) the source
        let mut ordered_exits: Vec<Entity> = Vec::new();
        let mut seen: HashSet<Entity> = HashSet::new();
        for &leaf in current_state.active_leaves.iter() {
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

        let is_internal = matches!(kind_query.get(trigger.event().edge), Ok(transitions::EdgeKind::Internal));
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
        let Some(exiting_leaf_state) = current_state.active_leaves.iter().find(|leaf| {
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
        let is_internal = matches!(kind_query.get(trigger.event().edge), Ok(transitions::EdgeKind::Internal));
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

    // Invoke typed Exit payload once at the start (root + source)
    trigger.event().payload.on_exit(&mut commands, source_state, &children_query, &current_state);
    for entity in states_to_exit_vec.iter() {
        // Save history if this state has history behavior
        if let Ok(history) = history_query.get(*entity) {
            let states_to_save = match history {
                History::Shallow => {
                    // For shallow history, save the immediate child of `entity` on the path
                    // to each active leaf descendant (handles both normal and parallel parents).
                    let mut saved: HashSet<Entity> = HashSet::new();
                    for &leaf in current_state.active_leaves.iter() {
                        // Track the previous node while walking ancestors; when we hit `entity`,
                        // `prev` is the immediate child under `entity`.
                        let mut prev = leaf;
                        for ancestor in child_of_query.iter_ancestors(leaf) {
                            if ancestor == *entity {
                                saved.insert(prev);
                                break;
                            }
                            prev = ancestor;
                        }
                    }
                    saved
                }
                History::Deep => {
                    // For deep history, save all active descendant leaves
                    current_state.active_leaves.iter()
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
        current_state.active_leaves.remove(exited);
    }

    // Transition actions phase (between exits and entries)
    commands.trigger_targets(TransitionActions, trigger.event().edge);
    trigger.event().payload.on_effect(&mut commands, trigger.event().edge, &children_query, &current_state);
    // Invoke typed Effect payload if present
    // Note: we avoid trait bounds here; user code can downcast payload if desired via helper

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
    current_state.active_leaves.extend(new_leaf_states);
    // Invoke typed Entry payload
    trigger.event().payload.on_entry(&mut commands, new_super_state, &children_query, &current_state);
    // Derive full active set from leaves
    current_state.active = compute_active_from_leaves(&current_state.active_leaves, &child_of_query);
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

        // 1) History takes precedence (works for both parallel and non-parallel parents)
        if let (Ok(history), Ok(history_state)) = (history_query.get(entity), history_state_query.get(entity)) {
            found_next = true;
            match history {
                History::Shallow => {
                    for &saved_state in &history_state.0 {
                        commands.trigger_targets(EnterState, saved_state);
                        stack.push(saved_state);
                    }
                }
                History::Deep => {
                    for &saved_state in &history_state.0 {
                        let mut path_to_substate = vec![saved_state];
                        path_to_substate.extend(
                            child_of_query
                                .iter_ancestors(saved_state)
                                .take_while(|&ancestor| ancestor != entity),
                        );
                        for e in path_to_substate.iter().rev() {
                            commands.trigger_targets(EnterState, *e);
                        }
                        leaves.insert(saved_state);
                    }
                    continue;
                }
            }
        }
        // 2) If it's a parallel state (without history), explore all children regions.
        else if parallel_query.get(entity).is_ok() {
            if let Ok(children) = children_query.get(entity) {
                found_next = true;
                for &child in children {
                    commands.trigger_targets(EnterState, child);
                    stack.push(child);
                }
            }
        }
        // 3) If it has a single initial state, explore that.
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

fn compute_active_from_leaves(
    leaves: &HashSet<Entity>,
    child_of_query: &Query<&StateChildOf>,
) -> HashSet<Entity> {
    let mut active: HashSet<Entity> = HashSet::new();
    for &leaf in leaves.iter() {
        active.insert(leaf);
        for ancestor in child_of_query.iter_ancestors(leaf) {
            active.insert(ancestor);
        }
    }
    active
}

/// Triggers the InitializeMachine event when AbilityMachine component is added.
fn initialize_state_machine(
    trigger: Trigger<OnAdd, StateMachine>,
    mut commands: Commands,
) {
    let target = trigger.target();
    // Always attempt to initialize: root-as-leaf, parallel, or parent with InitialState
    commands.trigger_targets(Transition { source: target, edge: target, payload: () }, target);
}

/// Resets a machine by clearing Active components under the root and re-inserting StateMachine
fn reset_state_region(
    trigger: Trigger<ResetRegion>,
    mut commands: Commands,
    children_query: Query<&StateChildren>,
) {
    let root = trigger.target();

    for child in children_query.iter_descendants(root) {
        commands.entity(child).remove::<Active>().insert(Inactive);
        commands.trigger_targets(prelude::Reset, child);
    }

    commands.entity(root).remove::<StateMachine>().insert(StateMachine::new());
}