#![feature(associated_type_defaults)]

use bevy::{prelude::*, reflect::Reflect};
use bevy::platform::collections::HashSet;

use crate::{active::{Active, Inactive}, guards::Guards, history::{History, HistoryState}};

pub mod active;
pub mod guards;
pub mod history;
pub mod prelude;
pub mod parameter;
pub mod state_component;
pub mod transitions;
pub mod bevy_state;

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
#[derive(EntityEvent)]
pub struct Transition<T = ()> where T: Clone + Send + Sync + 'static {
    #[event_target]
    pub machine: Entity,
    /// The state that triggered this transition. This is used to determine the scope
    /// of the transition, especially in parallel state machines.
    pub source: Entity,
    /// The transition edge entity that defines the target and kind.
    pub edge: Entity,
    /// Optional typed payload that can be used by the transition observer
    pub payload: T,
}

#[derive(EntityEvent, Reflect)]
pub struct TransitionActions { #[event_target] pub target: Entity }

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
    #[reflect(ignore)] #[entities]
    pub active: HashSet<Entity>,
    #[reflect(ignore)] #[entities]
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
#[derive(EntityEvent, Reflect)]
pub struct EnterState { #[event_target] pub target: Entity }

/// An event that is triggered on a state entity when it is being exited.
#[derive(EntityEvent, Reflect)]
pub struct ExitState { #[event_target] pub target: Entity }

/// Event to reset a state machine: clear Active flags under the root and reinitialize
#[derive(EntityEvent, Reflect)]
pub struct ResetRegion { #[event_target] pub target: Entity }

impl ResetRegion {
    pub fn new(entity: Entity) -> Self { Self { target: entity } }
}

/// The core system that observes `Transition` events and orchestrates the state change.
/// It calculates the exit and entry paths, sends `ExitState` and `EnterState` events
/// to the appropriate states, and updates the machine's `CurrentState`.
/// Also handles history state saving and restoration.
fn transition_observer<T: transitions::PhasePayload>(
    transition: On<Transition<T>>,
    mut q_sm: Query<&mut StateMachine>,
    q_parallel: Query<&Parallel>,
    q_children: Query<&StateChildren>,
    q_initial_state: Query<&InitialState>,
    q_history: Query<&History>,
    mut q_history_state: Query<&mut HistoryState>,
    q_child_of: Query<&StateChildOf>,
    q_edge_target: Query<&transitions::Target>,
    q_kind: Query<&transitions::EdgeKind>,
    mut commands: Commands,
) {
    let machine_entity = transition.event().machine;
    let source_state = transition.event().source;
    // Resolve target: prefer Target on the edge; otherwise treat the edge itself
    // as the super state to start from (useful for root init where initial is on the state).
    let new_super_state = match q_edge_target.get(transition.event().edge) {
        Ok(edge_target) => edge_target.0,
        Err(_) => transition.event().edge,
    };

    let Ok(mut current_state) = q_sm.get_mut(machine_entity) else {
        return;
    };

    // Handle initialization case where there are no current active states
    if current_state.active_leaves.is_empty() {
        // Build path from target up to (but excluding) the machine root
        let mut path_to_target: Vec<Entity> = vec![new_super_state];
        path_to_target.extend(
            q_child_of
                .iter_ancestors(new_super_state)
                .take_while(|&ancestor| ancestor != machine_entity),
        );

        // Enter ancestors parent→child down to the target
        for entity in path_to_target.iter().rev() {
            commands.trigger(EnterState { target: *entity });
        }

        let new_leaf_states = get_all_leaf_states(
            new_super_state,
            &q_initial_state,
            &q_children,
            &q_parallel,
            &q_history,
            &q_history_state,
            &q_child_of,
            &mut commands,
        );
        current_state.active_leaves.extend(new_leaf_states);
        // Derive full active set from leaves
        current_state.active = compute_active_from_leaves(&current_state.active_leaves, &q_child_of);
        return;
    }

    // Determine whether the source is a parallel parent (transition defined on a parallel state)
    let source_is_parallel = q_parallel.get(source_state).is_ok();

    // Exit/enter computation diverges for parallel parents
    let (states_to_exit_vec, states_to_enter_vec) = if source_is_parallel {
        // 1) Exit: all active leaves under the parallel source up to (and including) the source
        let mut ordered_exits: Vec<Entity> = Vec::new();
        let mut seen: HashSet<Entity> = HashSet::new();
        for &leaf in current_state.active_leaves.iter() {
            // Consider only leaves that are descendants of the parallel source
            let is_descendant = leaf == source_state
                || q_child_of.iter_ancestors(leaf).any(|a| a == source_state);
            if !is_descendant { continue; }

            // Exit path from leaf up to source_state (inclusive)
            let path = get_path_to_root(leaf, &q_child_of);
            if let Some(pos) = path.iter().position(|&e| e == source_state) {
                let slice = &path[..=pos]; // includes source_state
                for &e in slice {
                    if seen.insert(e) { ordered_exits.push(e); }
                }
            }
        }

        // 2) Enter: compute LCA between source_state and new_super_state
        let exit_path_from_source = get_path_to_root(source_state, &q_child_of);
        let enter_path = get_path_to_root(new_super_state, &q_child_of);

        let mut lca_depth = exit_path_from_source
            .iter()
            .rev()
            .zip(enter_path.iter().rev())
            .take_while(|(a, b)| a == b)
            .count();

        let lca_entity = if lca_depth > 0 { Some(exit_path_from_source[exit_path_from_source.len() - lca_depth]) } else { None };

        let is_internal = matches!(q_kind.get(transition.event().edge), Ok(transitions::EdgeKind::Internal));
        if !is_internal {
            // If source is the LCA, default external re-enters the source
            if lca_entity == Some(source_state) {
                lca_depth = lca_depth.saturating_sub(1);
            }
        }

        let states_to_enter = enter_path[..enter_path.len() - lca_depth].to_vec();
        (ordered_exits, states_to_enter)
    } else {
        // Non-parallel source: may still have multiple active descendant leaves if there are
        // deeper parallel regions underneath. Exit ALL active descendant leaves under `source_state`.
        let mut descendant_leaves: Vec<Entity> = current_state
            .active_leaves
            .iter()
            .copied()
            .filter(|leaf| {
                *leaf == source_state
                    || q_child_of
                        .iter_ancestors(*leaf)
                        .any(|ancestor| ancestor == source_state)
            })
            .collect();

        if descendant_leaves.is_empty() {
            // This transition is not coming from any of the currently active states.
            return;
        }

        let enter_path = get_path_to_root(new_super_state, &q_child_of);
        let is_internal = matches!(q_kind.get(transition.event().edge), Ok(transitions::EdgeKind::Internal));

        // Build ordered exits by walking each leaf up to (but not including) the LCA with the target path
        let mut ordered_exits: Vec<Entity> = Vec::new();
        let mut seen: HashSet<Entity> = HashSet::new();
        let mut min_lca_depth: Option<usize> = None;

        for leaf in descendant_leaves.drain(..) {
            let exit_path = get_path_to_root(leaf, &q_child_of);
            let mut lca_depth = exit_path
                .iter()
                .rev()
                .zip(enter_path.iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            let lca_entity = if lca_depth > 0 { Some(exit_path[exit_path.len() - lca_depth]) } else { None };

            if !is_internal {
                if new_super_state == leaf {
                    lca_depth = lca_depth.saturating_sub(1);
                } else if lca_entity == Some(source_state) {
                    lca_depth = lca_depth.saturating_sub(1);
                }
            }

            // Track minimal lca_depth across all leaves to compute entry path later
            min_lca_depth = Some(match min_lca_depth {
                Some(min) => min.min(lca_depth),
                None => lca_depth,
            });

            // Exit from leaf up to (but not including) the LCA portion
            let upto = exit_path.len() - lca_depth;
            for &e in &exit_path[..upto] {
                if seen.insert(e) {
                    ordered_exits.push(e);
                }
            }
        }

        let lca_depth_final = min_lca_depth.unwrap_or(0);
        let states_to_enter = enter_path[..enter_path.len() - lca_depth_final].to_vec();
        (ordered_exits, states_to_enter)
    };

    // Invoke typed Exit payload once at the start (root + source)
    transition.event().payload.on_exit(&mut commands, source_state, &q_children, &current_state);
    for entity in states_to_exit_vec.iter() {
        // Save history if this state has history behavior
        if let Ok(history) = q_history.get(*entity) {
            let states_to_save = match history {
                History::Shallow => {
                    // For shallow history, save the immediate child of `entity` on the path
                    // to each active leaf descendant (handles both normal and parallel parents).
                    let mut saved: HashSet<Entity> = HashSet::new();
                    for &leaf in current_state.active_leaves.iter() {
                        // Track the previous node while walking ancestors; when we hit `entity`,
                        // `prev` is the immediate child under `entity`.
                        let mut prev = leaf;
                        for ancestor in q_child_of.iter_ancestors(leaf) {
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
                            state == *entity || q_child_of
                                .iter_ancestors(state)
                                 .any(|ancestor| ancestor == *entity)
                        })
                        .copied()
                        .collect()
                }
            };
            
            // Insert or update the history state
            if let Ok(mut existing_history) = q_history_state.get_mut(*entity) {
                existing_history.0 = states_to_save;
            } else {
                commands.entity(*entity).insert(HistoryState(states_to_save));
            }
        }
        
        commands.trigger(ExitState { target: *entity });
    }

    // Update the current state set
    // For parallel parents we potentially exited multiple leaves; remove any leaves we exited.
    for exited in states_to_exit_vec.iter() {
        // Only remove if it was a leaf previously
        current_state.active_leaves.remove(exited);
    }

    // Transition actions phase (between exits and entries)
    commands.trigger(TransitionActions { target: transition.event().edge });
    transition.event().payload.on_effect(&mut commands, transition.event().edge, &q_children, &current_state);
    // Invoke typed Effect payload if present
    // Note: we avoid trait bounds here; user code can downcast payload if desired via helper

    // Enter from parent to child
    for entity in states_to_enter_vec.iter().rev() {
        commands.trigger(EnterState { target: *entity });
    }

    // Now, from the entered super state, drill down to the new leaf states.
    let new_leaf_states = get_all_leaf_states(
        new_super_state,
        &q_initial_state,
        &q_children,
        &q_parallel,
        &q_history,
        &q_history_state,
        &q_child_of,
        &mut commands,
    );
    current_state.active_leaves.extend(new_leaf_states);
    // Invoke typed Entry payload
    transition.event().payload.on_entry(&mut commands, new_super_state, &q_children, &current_state);
    // Derive full active set from leaves
    current_state.active = compute_active_from_leaves(&current_state.active_leaves, &q_child_of);
}

fn get_path_to_root(start_entity: Entity, q_child_of: &Query<&StateChildOf>) -> Vec<Entity> {
    let mut path = vec![start_entity];
    path.extend(q_child_of.iter_ancestors(start_entity));
    path
}

pub fn get_all_leaf_states(
    start_node: Entity,
    q_initial_state: &Query<&InitialState>,
    q_children: &Query<&StateChildren>,
    q_parallel: &Query<&Parallel>,
    q_history: &Query<&History>,
    q_history_state: &Query<&mut HistoryState>,
    q_child_of: &Query<&StateChildOf>,
    commands: &mut Commands,
) -> HashSet<Entity> {

    let mut leaves = HashSet::new();
    let mut stack = vec![start_node];

    while let Some(entity) = stack.pop() {
        let mut found_next = false;

        // 1) History takes precedence (works for both parallel and non-parallel parents)
        if let (Ok(history), Ok(history_state)) = (q_history.get(entity), q_history_state.get(entity)) {
            found_next = true;
            match history {
                History::Shallow => {
                    for &saved_state in &history_state.0 {
                        commands.trigger(EnterState { target: saved_state });
                        stack.push(saved_state);
                    }
                }
                History::Deep => {
                    for &saved_state in &history_state.0 {
                        let mut path_to_substate = vec![saved_state];
                        path_to_substate.extend(
                            q_child_of
                                .iter_ancestors(saved_state)
                                .take_while(|&ancestor| ancestor != entity),
                        );
                        for e in path_to_substate.iter().rev() {
                            commands.trigger(EnterState { target: *e });
                        }
                        leaves.insert(saved_state);
                    }
                    continue;
                }
            }
        }
        // 2) If it's a parallel state (without history), explore all children regions.
        else if q_parallel.get(entity).is_ok() {
            if let Ok(children) = q_children.get(entity) {
                found_next = true;
                for &child in children {
                    commands.trigger(EnterState { target: child });
                    stack.push(child);
                }
            }
        }
        // 3) If it has a single initial state, explore that.
        else if let Ok(initial_state) = q_initial_state.get(entity) {
            found_next = true;

            // To enter a deeply nested initial state, we must first enter all of its parents
            // that are descendants of the current state (`entity`).
            let mut path_to_substate = vec![initial_state.0];
            path_to_substate.extend(
                q_child_of
                    .iter_ancestors(initial_state.0)
                    .take_while(|&ancestor| ancestor != entity),
            );

            // Enter from parent to child
            for e in path_to_substate.iter().rev() {
                commands.trigger(EnterState { target: *e });
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
    q_child_of: &Query<&StateChildOf>,
) -> HashSet<Entity> {
    let mut active: HashSet<Entity> = HashSet::new();
    for &leaf in leaves.iter() {
        active.insert(leaf);
        for ancestor in q_child_of.iter_ancestors(leaf) {
            active.insert(ancestor);
        }
    }
    active
}

/// Triggers the InitializeMachine event when AbilityMachine component is added.
fn initialize_state_machine(
    add: On<Add, StateMachine>,
    mut commands: Commands,
) {
    let target = add.event().entity;
    // Always attempt to initialize: root-as-leaf, parallel, or parent with InitialState
    commands.trigger(Transition { machine: target, source: target, edge: target, payload: () });
}

/// Resets a machine by clearing Active components under the root and re-inserting StateMachine
fn reset_state_region(
    reset_region: On<ResetRegion>,
    mut commands: Commands,
    q_children: Query<&StateChildren>,
) {
    let root = reset_region.target;

    for child in q_children.iter_descendants(root) {
        commands.entity(child).remove::<Active>().insert(Inactive);
    }

    commands.entity(root).remove::<StateMachine>().insert(StateMachine::new());
}