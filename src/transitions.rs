use std::marker::PhantomData;
use std::time::Duration;

use bevy::prelude::*;
use std::collections::HashSet;

use crate::{guards::Guards, EnterState, Transition, active::Active, StateChildOf, StateMachine, ExitState};

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
#[reflect(Component)]
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
#[derive(Component)]
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

/// A component that can be added to states to defer specific event types.
/// Events of type `E` that arrive while this state is active will be stored
/// and replayed when the state is exited.
#[derive(Component)]
pub struct DeferEvents<E: Event> {
    pub deferred: Option<E>,
}

impl<E: Event> Default for DeferEvents<E> {
    fn default() -> Self {
        Self { deferred: None }
    }
}

impl<E: Event> DeferEvents<E> {
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

/// On EnterState(source), evaluate AlwaysEdge transitions listed in `Transitions(source)` in order.
pub fn transition_always(
    trigger: Trigger<EnterState>,
    transitions_query: Query<&Transitions>,
    always_query: Query<(), With<AlwaysEdge>>,
    edge_target_query: Query<&Target>,
    guards_query: Query<&Guards>,
    child_of_query: Query<&StateChildOf>,
    mut commands: Commands,
){
    let source = trigger.target();
    let Ok(transitions) = transitions_query.get(source) else { return; };

    // Evaluate in order; fire the first allowed transition
    for edge in transitions.into_iter().copied() {
        if always_query.get(edge).is_err() { continue; }

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
    mut defer_query: Query<&mut DeferEvents<E>>,
    active_query: Query<(), With<Active>>,
    mut commands: Commands,
){
    let event = trigger.event();
    
    // If the event target is a machine root, propagate to active leaves and evaluate in one pass
    if let Ok(current) = current_state_query.get(trigger.target()) {
        let machine_root = trigger.target();
        let mut visited: HashSet<Entity> = HashSet::new();
        for &leaf in current.0.iter() {
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
                return;
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
    defer_query: &mut Query<&mut DeferEvents<E>>,
    active_query: &Query<(), With<Active>>,
    commands: &mut Commands,
) -> bool {
    // Check if this state should defer this event type
    if let Ok(mut defer_events) = defer_query.get_mut(source) {
        if active_query.get(source).is_ok() {
            // State is active and has defer component - defer the event
            defer_events.defer_event(event.clone());
            return true; // Event was handled (deferred)
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
    defer_query: &mut Query<&mut DeferEvents<E>>,
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
    guards_changed_query: Query<(Entity, &Guards, &Source, Has<Target>, Has<Active>), (Changed<Guards>, With<AlwaysEdge>)>, 
    transitions_query: Query<&Transitions>,
    child_of_query: Query<&StateChildOf>,
    mut commands: Commands,
) {
    for (edge, guards, source, edge_target, active) in guards_changed_query.iter() {
        let source = source.0;

        // Only consider Always edges whose guard set changed to passing
        if !guards.check() { continue; }

        // Find the source state for this edge and ensure it's active   
        if !active { continue; }

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

/// Generic system to replay deferred events when a state exits.
pub fn replay_deferred_events<E: Event + Clone>(
    trigger: Trigger<ExitState>,
    mut defer_query: Query<&mut DeferEvents<E>>,
    child_of_query: Query<&StateChildOf>,
    mut commands: Commands,
) {
    let exited_state = trigger.target();
    
    if let Ok(mut defer_events) = defer_query.get_mut(exited_state) {
        let deferred = defer_events.take_deferred();
        if let Some(deferred) = deferred {
            let root_entity = child_of_query.root_ancestor(exited_state);
            println!("   🔄 Replaying deferred event from state");
            
            // Replay all deferred events to the machine root
            commands.trigger_targets(deferred, root_entity);
        }
    }
}


