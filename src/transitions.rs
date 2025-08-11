use std::marker::PhantomData;
use std::time::Duration;

use bevy::prelude::*;
use bevy_ecs::{component::{Mutable, StorageType}, entity::MapEntities};

use crate::{guards::Guards, EnterState, Transition, active::Active, StateChildOf};

/// Outbound transitions from a source state. Order defines priority (first match wins).
#[derive(Reflect, Component)]
#[reflect(Component)]
#[relationship_target(relationship = Source, linked_spawn)]
pub struct Transitions(Vec<Entity>);

impl MapEntities for Transitions {
    fn map_entities<E: EntityMapper>(&mut self, entity_mapper: &mut E) {
        for edge in &mut self.0 {
            *edge = entity_mapper.get_mapped(*edge);
        }
    }
}

impl Transitions {
    pub fn new() -> Self {
        Self(vec![])
    }
    pub fn get_transitions(&self) -> &[Entity] {
        &self.0
    }
}

#[derive(Component, Reflect)]
#[reflect(Component)]
#[relationship(relationship_target = Transitions)]
pub struct Source(pub Entity);

impl MapEntities for Source {
    fn map_entities<E: EntityMapper>(&mut self, entity_mapper: &mut E) {
        self.0 = entity_mapper.get_mapped(self.0);
    }
}

/// Target for an edge transition.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct Target(pub Entity);

impl MapEntities for Target {
    fn map_entities<E: EntityMapper>(&mut self, entity_mapper: &mut E) {
        self.0 = entity_mapper.get_mapped(self.0);
    }
}

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
#[derive(Reflect)]
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

impl<T: Event> Component for TransitionListener<T> {
    const STORAGE_TYPE: StorageType = StorageType::Table;

    type Mutability = Mutable;
}

impl<E: Event> MapEntities for TransitionListener<E> {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}

/// On EnterState(source), evaluate AlwaysEdge transitions listed in `Transitions(source)` in order.
pub fn transition_edge_always(
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
    for edge in transitions.get_transitions().iter().copied() {
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
pub fn transition_edge_listener<E: Event>(
    trigger: Trigger<E>,
    transitions_query: Query<&Transitions>,
    listener_query: Query<&TransitionListener<E>>, 
    edge_target_query: Query<&Target>,
    guards_query: Query<&Guards>,
    child_of_query: Query<&StateChildOf>,
    mut commands: Commands,
){
    let source = trigger.target();
    let Ok(transitions) = transitions_query.get(source) else { return; };

    for edge in transitions.get_transitions().iter().copied() {
        if listener_query.get(edge).is_err() { continue; }

        // Evaluate guards on the edge itself if present
        if let Ok(guards) = guards_query.get(edge) {
            if !guards.check() { continue; }
        }

        // Resolve target
        if edge_target_query.get(edge).is_err() { continue; }

        // Fire transition
        let root = child_of_query.root_ancestor(source);

        commands.trigger_targets(Transition { source, edge }, root);
        break;
    }
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
        if !transitions.get_transitions().iter().any(|&e| e == edge) { continue; }

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
    for edge in transitions.get_transitions().iter().copied() {
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
    for edge in transitions.get_transitions().iter().copied() {
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
        for edge in transitions.get_transitions().iter().copied() {
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


