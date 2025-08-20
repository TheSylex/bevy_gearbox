use std::marker::PhantomData;
use std::time::Duration;

use bevy::prelude::*;
use std::collections::HashSet;

use crate::{guards::Guards, EnterState, Transition, active::Active, StateChildOf, StateMachine, ExitState, Parallel};
use crate::state_component::Reset;

/// Outbound transitions from a source state. Order defines priority (first match wins).
#[derive(Component, Default, Debug, PartialEq, Eq, Reflect)]
#[relationship_target(relationship = Source, linked_spawn)]
#[reflect(Component, FromWorld, Default)]
pub struct Transitions(Vec<Entity>);

impl<'a> IntoIterator for &'a Transitions {
    type Item = <Self::IntoIter as Iterator>::Item;

    type IntoIter = std::slice::Iter<'a, Entity>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl Transitions {
    pub fn new() -> Self {
        Self(Vec::new())
    }
}

#[derive(Component, Clone, PartialEq, Eq, Debug, Reflect)]
#[relationship(relationship_target = Transitions)]
#[reflect(Component, PartialEq, Debug, FromWorld, Clone)]
pub struct Source(#[entities] pub Entity);

impl FromWorld for Source {
    #[inline(always)]
    fn from_world(_world: &mut World) -> Self {
        Source(Entity::PLACEHOLDER)
    }
}

/// Target for an edge transition.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct Target(#[entities] pub Entity);

/// Whether the transition should be treated as External (default) or Internal.
#[derive(Component, Reflect, Default, Clone, Copy)]
#[reflect(Component, Default)]
pub enum TransitionKind { 
    #[default]
    External,
    Internal,
}

/// Marker for a transition that should fire on entering the source state (no event).
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct AlwaysEdge;

/// Delayed transition configuration: fire after `duration` has elapsed while the source is active.
#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct After {
    pub duration: Duration,
}

#[derive(Component)]
pub struct EdgeTimer(pub Timer);

/// Attach this to a transition entity to react to a specific event `E`.
#[derive(Reflect, Component)]
#[reflect(Component)]
pub struct TransitionListener<E: Event> {
    #[reflect(ignore)]
    _marker: PhantomData<E>,
}

impl<E: Event> Default for TransitionListener<E> {
    fn default() -> Self {
        Self { _marker: PhantomData }
    }
}

/// A component that can be added to states to an event of a specific type.
/// Event of type `E` that arrive while this state is active will be stored
/// and replayed when the state is exited.
#[derive(Component)]
pub struct DeferEvent<E: Event> {
    pub deferred: Option<E>,
}

impl<E: Event> Default for DeferEvent<E> {
    fn default() -> Self {
        Self { deferred: None }
    }
}

impl<E: Event> DeferEvent<E> {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn defer_event(&mut self, event: E) {
        self.deferred = Some(event);
    }
    
    pub fn take_deferred(&mut self) -> Option<E> {
        std::mem::take(&mut self.deferred)
    }
}

/// Marker to request reset of subtree(s) during TransitionActions phase
#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct ResetEdge(pub ResetScope);

#[derive(Reflect, Default, Clone, Copy)]
pub enum ResetScope { #[default] Source, Target, Both }

/// On EnterState(source), evaluate AlwaysEdge transitions listed in `Transitions(source)` in order.
/// Respects After components - transitions with After will be handled by the timer system instead.
pub fn transition_always(
    trigger: Trigger<EnterState>,
    transitions_query: Query<&Transitions>,
    always_query: Query<(), With<AlwaysEdge>>,
    edge_target_query: Query<&Target>,
    guards_query: Query<&Guards>,
    after_query: Query<&After>,
    child_of_query: Query<&StateChildOf>,
    mut commands: Commands,
){
    let source = trigger.target();
    let Ok(transitions) = transitions_query.get(source) else { return; };

    // Evaluate in order; fire the first allowed transition
    for edge in transitions.into_iter().copied() {
        if always_query.get(edge).is_err() { continue; }

        // Skip transitions with After component - let the timer system handle them
        if after_query.get(edge).is_ok() { continue; }

        // Resolve target from edge
        if edge_target_query.get(edge).is_err() { continue; }

        // Evaluate guards on the edge itself if present
        if let Ok(guards) = guards_query.get(edge) {
            if !guards.check() { continue; }
        }

        // Fire transition

        let root = child_of_query.root_ancestor(source);

        commands.trigger_targets(Transition { source, edge }, root);
        break;
    }
}

/// Helper function to find the parallel region root for a given state.
/// Returns the state itself if it's not under a parallel region.
fn find_parallel_region_root(
    state: Entity,
    child_of_query: &Query<&StateChildOf>,
    parallel_query: &Query<&Parallel>,
) -> Entity {
    // Walk up the hierarchy to find if we're under a parallel state
    let mut previous_ancestor = state;
    for ancestor in child_of_query.iter_ancestors(state) {
        if parallel_query.contains(ancestor) {
            return previous_ancestor;
        }
        previous_ancestor = ancestor;
    }

    // Not under a parallel state, return the state itself
    state
}

/// Generic listener: on event `E` at a source state, scan its `Transitions` for a matching
/// transition entity with `TransitionListener<E>`, in priority order.
pub fn transition_listener<E: Event + Clone>(
    trigger: Trigger<E>,
    transitions_query: Query<&Transitions>,
    listener_query: Query<&TransitionListener<E>>, 
    edge_target_query: Query<&Target>,
    guards_query: Query<&Guards>,
    child_of_query: Query<&StateChildOf>,
    current_state_query: Query<&StateMachine>,
    mut defer_query: Query<&mut DeferEvent<E>>,
    active_query: Query<(), With<Active>>,
    parallel_query: Query<&Parallel>,
    mut commands: Commands,
){
    let event = trigger.event();
    
    // If the event target is a machine root, propagate to active leaves and evaluate in one pass
    if let Ok(current) = current_state_query.get(trigger.target()) {
        let machine_root = trigger.target();
        let mut visited: HashSet<Entity> = HashSet::new();
        let mut fired_regions: HashSet<Entity> = HashSet::new();
        
        for &leaf in current.active_leaves.iter() {
            // Find which parallel region this leaf belongs to
            let region_root = find_parallel_region_root(
                leaf, 
                &child_of_query,
                &parallel_query,
            );
            
            // Skip if we've already fired a transition in this parallel region
            if fired_regions.contains(&region_root) {
                continue;
            }
            
            if try_fire_first_matching_edge_on_branch(
                leaf,
                event,
                machine_root,
                &transitions_query,
                &listener_query,
                &edge_target_query,
                &guards_query,
                &child_of_query,
                &mut defer_query,
                &active_query,
                &mut visited,
                &mut commands,
            ) {
                // Mark this parallel region as having fired a transition
                fired_regions.insert(region_root);
                // Don't return - continue to allow other parallel regions to fire
            }
        }
        return;
    }

    // Otherwise, evaluate on the targeted state directly
    let source = trigger.target();
    try_fire_first_matching_edge(
        source,
        event,
        &transitions_query,
        &listener_query,
        &edge_target_query,
        &guards_query,
        &child_of_query,
        &mut defer_query,
        &active_query,
        &mut commands,
    );
}

fn try_fire_first_matching_edge<E: Event + Clone>(
    source: Entity,
    event: &E,
    transitions_query: &Query<&Transitions>,
    listener_query: &Query<&TransitionListener<E>>, 
    edge_target_query: &Query<&Target>,
    guards_query: &Query<&Guards>,
    child_of_query: &Query<&StateChildOf>,
    defer_query: &mut Query<&mut DeferEvent<E>>,
    active_query: &Query<(), With<Active>>,
    commands: &mut Commands,
) -> bool {
    // Check if this state should defer this event type
    if let Ok(mut defer_event) = defer_query.get_mut(source) {
        if active_query.get(source).is_ok() {
            // State is active and has defer component - defer the event
            defer_event.defer_event(event.clone());
            return false; // Event was handled (deferred)
        }
    }

    let Ok(transitions) = transitions_query.get(source) else { return false; };

    for edge in transitions.into_iter().copied() {
        if listener_query.get(edge).is_err() { continue; }

        if let Ok(guards) = guards_query.get(edge) {
            if !guards.check() { continue; }
        }

        if edge_target_query.get(edge).is_err() { continue; }

        let root = child_of_query.root_ancestor(source);
        commands.trigger_targets(Transition { source, edge }, root);
        return true;
    }
    false
}

fn try_fire_first_matching_edge_on_branch<E: Event + Clone>(
    start: Entity,
    event: &E,
    machine_root: Entity,
    transitions_query: &Query<&Transitions>,
    listener_query: &Query<&TransitionListener<E>>, 
    edge_target_query: &Query<&Target>,
    guards_query: &Query<&Guards>,
    child_of_query: &Query<&StateChildOf>,
    defer_query: &mut Query<&mut DeferEvent<E>>,
    active_query: &Query<(), With<Active>>,
    visited: &mut HashSet<Entity>,
    commands: &mut Commands,
) -> bool {
    // Walk from leaf up to (but not beyond) the machine root
    let mut current = Some(start);
    while let Some(state) = current {
        // Skip states already checked across other branches
        if !visited.insert(state) {
            if state == machine_root { break; }
            current = child_of_query.get(state).ok().map(|rel| rel.0);
            continue;
        }
        if try_fire_first_matching_edge(
            state,
            event,
            transitions_query,
            listener_query,
            edge_target_query,
            guards_query,
            child_of_query,
            defer_query,
            active_query,
            commands,
        ) {
            return true;
        }
        if state == machine_root { break; }
        current = child_of_query.get(state).ok().map(|rel| rel.0);
    }
    false
}


/// When guards on an Always edge change while its source state is active, re-check and fire if now allowed.
pub fn check_always_on_guards_changed(
    guards_changed_query: Query<(Entity, &Guards, &Source, Has<Target>), (Changed<Guards>, With<AlwaysEdge>)>, 
    transitions_query: Query<&Transitions>,
    child_of_query: Query<&StateChildOf>,
    active_query: Query<(), With<Active>>,
    mut commands: Commands,
) {
    for (edge, guards, source, edge_target) in guards_changed_query.iter() {

        let source = source.0;

        if !active_query.contains(source) { continue; }

        // Only consider Always edges whose guard set changed to passing
        if !guards.check() { continue; }

        // Ensure this edge is actually listed on the source's transitions (priority set)
        let Ok(transitions) = transitions_query.get(source) else { continue; };
        if !transitions.into_iter().any(|&e| e == edge) { continue; }

        // Ensure edge has a valid target; then fire
        if !edge_target { continue; }
        let root = child_of_query.root_ancestor(source);

        commands.trigger_targets(Transition { source, edge }, root);
    }
}

/// On EnterState(source), start timers for any After edges.
pub fn start_after_on_enter(
    trigger: Trigger<EnterState>,
    transitions_query: Query<&Transitions>,
    after_query: Query<&After>,
    mut commands: Commands,
) {
    let source = trigger.target();
    let Ok(transitions) = transitions_query.get(source) else { return; };
    for edge in transitions.into_iter().copied() {
        if let Ok(after) = after_query.get(edge) {
            commands.entity(edge).insert(EdgeTimer(Timer::new(after.duration, TimerMode::Once)));
        }
    }
}

/// On ExitState(source), cancel timers for any After edges.
pub fn cancel_after_on_exit(
    trigger: Trigger<crate::ExitState>,
    transitions_query: Query<&Transitions>,
    after_query: Query<&After>,
    mut commands: Commands,
) {
    let source = trigger.target();
    let Ok(transitions) = transitions_query.get(source) else { return; };
    for edge in transitions.into_iter().copied() {
        if after_query.get(edge).is_ok() {
            commands.entity(edge).remove::<EdgeTimer>();
        }
    }
}

/// During TransitionActions, if an edge has ResetEdge, emit ResetSubtree for its scope
pub(crate) fn reset_on_transition_actions(
    trigger: Trigger<crate::TransitionActions>,
    reset_edge_q: Query<&ResetEdge>,
    edge_q: Query<(&Source, &Target)>,
    children_q: Query<&crate::StateChildren>,
    mut commands: Commands,
) {
    let edge = trigger.target();
    let Ok(reset) = reset_edge_q.get(edge) else { return; };
    
    let Ok((Source(source), Target(target))) = edge_q.get(edge) else { return; };

    let mut entities = vec![];

    match reset.0 {
        ResetScope::Source => {
            entities.push(*source);
            entities.extend(children_q.iter_descendants(*source));
        }
        ResetScope::Target => {
            entities.push(*target);
            entities.extend(children_q.iter_descendants(*target));
        }
        ResetScope::Both => {
            entities.push(*source);
            entities.push(*target);
            entities.extend(children_q.iter_descendants(*source));
            entities.extend(children_q.iter_descendants(*target));
        }
    }

    for entity in entities {
        println!("Resetting entity: {:?}", entity);
        commands.trigger_targets(Reset, entity);
    }
}

/// Tick After timers and fire the first due transition per active source, respecting Transitions order.
pub fn tick_after_system(
    time: Res<Time>,
    sources_with_transitions: Query<(Entity, &Transitions), With<Active>>, // active source states only
    mut timer_query: Query<&mut EdgeTimer>,
    after_query: Query<&After>,
    guards_query: Query<&Guards>,
    edge_target_query: Query<&Target>,
    child_of_query: Query<&StateChildOf>,
    mut commands: Commands,
) {
    for (source, transitions) in sources_with_transitions.iter() {
        // Walk edges in priority order; fire first eligible
        for edge in transitions.into_iter().copied() {
            if after_query.get(edge).is_err() { continue; }
            let Ok(mut timer) = timer_query.get_mut(edge) else { continue; };
            timer.0.tick(time.delta());
            if !timer.0.just_finished() { continue; }

            // Guards on edge (optional)
            if let Ok(guards) = guards_query.get(edge) {
                if !guards.check() { continue; }
            }
            // Need a valid target
            if edge_target_query.get(edge).is_err() { continue; }

            // Cancel timer to avoid multiple firings if state persists
            commands.entity(edge).remove::<EdgeTimer>();

            // Emit transition to the machine root
            let root = child_of_query.root_ancestor(source);
            commands.trigger_targets(Transition { source, edge }, root);
            break; // only one delayed transition per source per frame
        }
    }
}

/// Generic system to replay deferred event when a state exits.
pub fn replay_deferred_event<E: Event + Clone>(
    trigger: Trigger<ExitState>,
    mut defer_query: Query<&mut DeferEvent<E>>,
    child_of_query: Query<&StateChildOf>,
    mut commands: Commands,
) {
    let exited_state = trigger.target();
    
    if let Ok(mut defer_event) = defer_query.get_mut(exited_state) {
        let deferred = defer_event.take_deferred();
        if let Some(deferred) = deferred {
            let root_entity = child_of_query.root_ancestor(exited_state);
            
            // Replay all deferred event to the machine root
            commands.trigger_targets(deferred, root_entity);
        }
    }
}