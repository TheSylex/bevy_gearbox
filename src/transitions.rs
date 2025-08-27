use std::marker::PhantomData;
use std::time::Duration;

use bevy::prelude::*;
use std::collections::HashSet;

use crate::StateChildren;
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
pub enum EdgeKind { 
    #[default]
    External,
    Internal,
}

/// Marker for a transition that should fire on entering the source state (no event).
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(EdgeKind)]
pub struct AlwaysEdge;

/// Delayed transition configuration: fire after `duration` has elapsed while the source is active.
#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct After {
    pub duration: Duration,
}

impl After {
    #[inline]
    pub fn new(duration: Duration) -> Self { Self { duration } }
}

#[derive(Component)]
pub struct EdgeTimer(pub Timer);

/// Pending event stored on an edge awaiting its After timer
#[derive(Component)]
pub struct PendingEvent<E: Event + Clone> {
    pub event: E,
}

/// Marker event to represent absence of a payload
#[derive(Event, Reflect, Clone, Default)]
#[reflect(Default)]
pub struct NoEvent;

/// Derive macro for simple events that don't need phase-specific payloads
pub use bevy_gearbox_macros::SimpleTransition;

fn cleanup_edge_timer_and_pending<E: Event + Clone + 'static>(
    commands: &mut Commands,
    edge: Entity,
) {
    commands.entity(edge).remove::<EdgeTimer>().remove::<PendingEvent<E>>();
}

/// Trait implemented by events that can provide phase-specific payloads
/// for the transition lifecycle. All associated types default to NoEvent,
/// and all getters default to None.
pub trait TransitionEvent: Event + Clone {
    type ExitEvent: Event + Clone = NoEvent;
    type EffectEvent: Event + Clone = NoEvent;
    type EntryEvent: Event + Clone = NoEvent;

    fn to_exit_event(&self) -> Option<Self::ExitEvent> { None }
    fn to_effect_event(&self) -> Option<Self::EffectEvent> { None }
    fn to_entry_event(&self) -> Option<Self::EntryEvent> { None }
}

// Note: No blanket impl for TransitionEvent to avoid conflicting impls in downstream crates.

/// A typed phase payload holder built from a TransitionEvent
#[derive(Clone, Default)]
pub struct PhaseEvents<Exit: Event + Clone = NoEvent, Effect: Event + Clone = NoEvent, Entry: Event + Clone = NoEvent> {
    pub exit: Option<Exit>,
    pub effect: Option<Effect>,
    pub entry: Option<Entry>,
}

/// Phase callbacks the transition observer will invoke at microsteps
pub trait PhasePayload: Clone + Send + Sync + 'static {
    fn on_exit(&self, _commands: &mut Commands, _source: Entity, _children: &Query<&StateChildren>) {}
    fn on_effect(&self, _commands: &mut Commands, _edge: Entity, _children: &Query<&StateChildren>) {}
    fn on_entry(&self, _commands: &mut Commands, _target: Entity, _children: &Query<&StateChildren>) {}
}

impl PhasePayload for () {}

impl<Exit: Event + Clone, Effect: Event + Clone, Entry: Event + Clone> PhasePayload for PhaseEvents<Exit, Effect, Entry> {
    fn on_exit(&self, commands: &mut Commands, source: Entity, children: &Query<&StateChildren>) {
        if let Some(ev) = self.exit.clone() { commands.trigger_targets(ev, source); }
        for child in children.iter_descendants(source) {
            if let Some(ev) = self.exit.clone() { commands.trigger_targets(ev, child); }
        }
    }
    fn on_effect(&self, commands: &mut Commands, edge: Entity, children: &Query<&StateChildren>) {
        if let Some(ev) = self.effect.clone() { commands.trigger_targets(ev, edge); }
        for child in children.iter_descendants(edge) {
            if let Some(ev) = self.effect.clone() { commands.trigger_targets(ev, child); }
        }
    }
    fn on_entry(&self, commands: &mut Commands, target: Entity, children: &Query<&StateChildren>) {
        if let Some(ev) = self.entry.clone() { commands.trigger_targets(ev, target); }
        for child in children.iter_descendants(target) {
            if let Some(ev) = self.entry.clone() { commands.trigger_targets(ev, child); }
        }
    }
}

/// App extension to register transition event support
pub trait TransitionEventAppExt {
    fn add_transition_event<E: TransitionEvent + Clone + 'static>(&mut self) -> &mut Self;
}

impl TransitionEventAppExt for App {
    fn add_transition_event<E: TransitionEvent + Clone + 'static>(&mut self) -> &mut Self {
        self.add_observer(edge_event_listener::<E>)
            .add_observer(crate::transition_observer::<PhaseEvents<E::ExitEvent, E::EffectEvent, E::EntryEvent>>)
            .add_systems(Update, tick_after_event_timers::<E>)
            .add_observer(cancel_pending_event_on_exit::<E>)
    }
}

/// Generic edge firing logic for TransitionEvent
fn try_fire_first_matching_edge_generic<E: TransitionEvent + Clone>(
    source: Entity,
    event: &E,
    transitions_query: &Query<&Transitions>,
    listener_query: &Query<&EventEdge<E>>, 
    edge_target_query: &Query<&Target>,
    guards_query: &Query<&Guards>,
    child_of_query: &Query<&StateChildOf>,
    defer_query: &mut Query<&mut DeferEvent<E>>,
    active_query: &Query<(), With<Active>>,
    after_query: &Query<&After>,
    timer_query: &mut Query<&mut EdgeTimer>,
    commands: &mut Commands,
) -> bool {
    // Check if this state should defer this event type
    if let Ok(mut defer_event) = defer_query.get_mut(source) {
        if active_query.get(source).is_ok() {
            defer_event.defer_event(event.clone());
            return false;
        }
    }

    let Ok(transitions) = transitions_query.get(source) else { return false; };

    for edge in transitions.into_iter().copied() {
        if listener_query.get(edge).is_err() { continue; }

        // If edge is delayed, schedule timer and store pending event
        if let Ok(after) = after_query.get(edge) {
            if let Ok(mut timer) = timer_query.get_mut(edge) {
                timer.0.set_duration(after.duration);
                timer.0.reset();
            } else {
                commands.entity(edge).insert(EdgeTimer(Timer::new(after.duration, TimerMode::Once)));
            }
            commands.entity(edge).insert(PendingEvent::<E> { event: event.clone() });
            return true;
        }

        let payload = PhaseEvents {
            exit: event.to_exit_event(),
            effect: event.to_effect_event(),
            entry: event.to_entry_event(),
        };
        let root = child_of_query.root_ancestor(source);
        commands.trigger_targets(Transition { source, edge, payload }, root);
        return true;
    }
    false
}



/// Attach this to a transition entity to react to a specific event `E`.
#[derive(Reflect, Component)]
#[reflect(Component)]
#[require(EdgeKind)]
pub struct EventEdge<E: Event> {
    #[reflect(ignore)]
    _marker: PhantomData<E>,
}

impl<E: Event> Default for EventEdge<E> {
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
pub fn always_edge_listener(
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

        // Fire transition
        let root = child_of_query.root_ancestor(source);
        commands.trigger_targets(Transition { source, edge, payload: () }, root);
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

/// On event `E`, scan `Transitions` for a matching edge with `EventEdge<E>`, in priority order.
fn edge_event_listener<E: TransitionEvent + Clone>(
    trigger: Trigger<E>,
    transitions_query: Query<&Transitions>,
    listener_query: Query<&EventEdge<E>>, 
    edge_target_query: Query<&Target>,
    guards_query: Query<&Guards>,
    child_of_query: Query<&StateChildOf>,
    current_state_query: Query<&StateMachine>,
    mut defer_query: Query<&mut DeferEvent<E>>,
    active_query: Query<(), With<Active>>,
    parallel_query: Query<&Parallel>,
    after_query: Query<&After>,
    mut timer_query: Query<&mut EdgeTimer>,
    mut commands: Commands,
){
    let event = trigger.event();
    
    // If the event target is a machine root, first try root transitions, then propagate to active leaves
    if let Ok(current) = current_state_query.get(trigger.target()) {
        let machine_root = trigger.target();
        let mut visited: HashSet<Entity> = HashSet::new();
        let mut fired_regions: HashSet<Entity> = HashSet::new();
        
        // First, try to fire transitions on the root itself
        if try_fire_first_matching_edge(
            machine_root, event, &transitions_query, &listener_query, &edge_target_query,
            &guards_query, &child_of_query, &mut defer_query, &active_query, 
            &after_query, &mut timer_query, &mut commands,
        ) {
            return; // Root transition fired, don't propagate to leaves
        }
        
        // If no root transition fired, propagate to active leaves
        for &leaf in current.active_leaves.iter() {
            let region_root = find_parallel_region_root(leaf, &child_of_query, &parallel_query);
            if fired_regions.contains(&region_root) { continue; }
            
            if try_fire_first_matching_edge_on_branch(
                leaf, event, machine_root,
                &transitions_query, &listener_query, &edge_target_query, &guards_query,
                &child_of_query, &mut defer_query, &active_query, &after_query, 
                &mut timer_query, &mut visited, &mut commands,
            ) {
                fired_regions.insert(region_root);
            }
        }
        return;
    }

    // Otherwise, evaluate on the targeted state directly
    let source = trigger.target();
    try_fire_first_matching_edge(
        source, event, &transitions_query, &listener_query, &edge_target_query,
        &guards_query, &child_of_query, &mut defer_query, &active_query, 
        &after_query, &mut timer_query, &mut commands,
    );
}

fn try_fire_first_matching_edge<E: TransitionEvent + Clone>(
    source: Entity,
    event: &E,
    transitions_query: &Query<&Transitions>,
    listener_query: &Query<&EventEdge<E>>, 
    edge_target_query: &Query<&Target>,
    guards_query: &Query<&Guards>,
    child_of_query: &Query<&StateChildOf>,
    defer_query: &mut Query<&mut DeferEvent<E>>,
    active_query: &Query<(), With<Active>>,
    after_query: &Query<&After>,
    timer_query: &mut Query<&mut EdgeTimer>,
    commands: &mut Commands,
) -> bool {
    try_fire_first_matching_edge_generic(
        source, event, transitions_query, listener_query, edge_target_query,
        guards_query, child_of_query, defer_query, active_query, after_query,
        timer_query, commands,
    )
}

fn try_fire_first_matching_edge_on_branch<E: Event + Clone + TransitionEvent>(
    start: Entity,
    event: &E,
    machine_root: Entity,
    transitions_query: &Query<&Transitions>,
    listener_query: &Query<&EventEdge<E>>, 
    edge_target_query: &Query<&Target>,
    guards_query: &Query<&Guards>,
    child_of_query: &Query<&StateChildOf>,
    defer_query: &mut Query<&mut DeferEvent<E>>,
    active_query: &Query<(), With<Active>>,
    after_query: &Query<&After>,
    timer_query: &mut Query<&mut EdgeTimer>,
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
            after_query,
            timer_query,
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

        commands.trigger_targets(Transition { source, edge, payload: () }, root);
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

            // Cancel timer to avoid multiple firings if state persists
            commands.entity(edge).remove::<EdgeTimer>();

            // Emit transition to the machine root with empty payload
            let root = child_of_query.root_ancestor(source);
            commands.trigger_targets(Transition { source, edge, payload: () }, root);
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

/// Timer system for event edges with After; fire when due
pub fn tick_after_event_timers<E: TransitionEvent + Clone + 'static>(
    time: Res<Time>,
    mut timer_query: Query<(Entity, &mut EdgeTimer, &PendingEvent<E>), With<EventEdge<E>>>,
    after_query: Query<&After>,
    guards_query: Query<&Guards>,
    edge_target_query: Query<&Target>,
    edge_source_query: Query<&Source>,
    child_of_query: Query<&StateChildOf>,
    active_query: Query<(), With<Active>>,
    mut commands: Commands,
) {
    for (edge, mut timer, pending) in timer_query.iter_mut() {
        // Only consider edges that still have After
        if after_query.get(edge).is_err() { continue; }

        // If the source is no longer active, cancel the pending event
        let Ok(Source(source)) = edge_source_query.get(edge) else { continue; };
        if active_query.get(*source).is_err() {
            cleanup_edge_timer_and_pending::<E>(&mut commands, edge);
            continue;
        }

        timer.0.tick(time.delta());
        if !timer.0.just_finished() { continue; }

        let payload = PhaseEvents {
            exit: pending.event.to_exit_event(),
            effect: pending.event.to_effect_event(),
            entry: pending.event.to_entry_event(),
        };

        // Cleanup timer/pending and fire the transition to machine root
        cleanup_edge_timer_and_pending::<E>(&mut commands, edge);
        let root = child_of_query.root_ancestor(*source);
        commands.trigger_targets(Transition { source: *source, edge, payload }, root);
    }
}



/// Cancel a pending delayed event for a source when it exits
pub fn cancel_pending_event_on_exit<E: Event + Clone + 'static>(
    trigger: Trigger<ExitState>,
    transitions_query: Query<&Transitions>,
    pending_query: Query<&PendingEvent<E>>,
    mut commands: Commands,
){
    let source = trigger.target();
    let Ok(transitions) = transitions_query.get(source) else { return; };
    for &edge in transitions.into_iter() {
        if pending_query.get(edge).is_ok() {
            cleanup_edge_timer_and_pending::<E>(&mut commands, edge);
        }
    }
}