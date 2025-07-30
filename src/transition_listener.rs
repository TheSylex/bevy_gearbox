use std::marker::PhantomData;
use bevy::{prelude::*, reflect::Reflect};
use bevy_ecs::{component::{Mutable, StorageType}, entity::MapEntities};
use crate::{find_state_machine_root, Connection, StateMachineRoot, Transition};

/// A component that listens for a specific event `E` and triggers a `Transition`
/// when the event occurs on this entity.
#[derive(Reflect)]
#[reflect(Component)]
pub struct TransitionListener<E: Event> {
    connection: Connection,
    #[reflect(ignore)]
    _marker: PhantomData<E>,
}

impl<T: Event> Component for TransitionListener<T> {
    const STORAGE_TYPE: StorageType = StorageType::Table;

    type Mutability = Mutable;

    fn map_entities<E: EntityMapper>(this: &mut Self, entity_mapper: &mut E) {
        this.connection.map_entities(entity_mapper);
    }
}

impl<E: Event> TransitionListener<E> {
    pub fn new(connection: Connection) -> Self {
        Self {
            connection,
            _marker: PhantomData,
        }
    }
}

impl<E: Event> MapEntities for TransitionListener<E> {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        self.connection.map_entities(entity_mapper);
    }
}

/// A system that handles events for entities with a `TransitionListener`.
/// When an event `E` is triggered on an entity with a `TransitionListener<E>`,
/// this system fires a `Transition` event targeting the state machine's root.
pub fn transition_listener<E: Event>(
    trigger: Trigger<E>,
    listener_query: Query<&TransitionListener<E>>,
    child_of_query: Query<&ChildOf>,
    state_machine_root_query: Query<&StateMachineRoot>,
    mut commands: Commands,
) {
    let target = trigger.target();
    let Ok(listener) = listener_query.get(target) else {
        return;
    };

    let root_entity = find_state_machine_root(target, &child_of_query, &state_machine_root_query);

    let Some(root_entity) = root_entity else {
        return;
    };

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
pub trait ComplexTransitionListener: Component + Reflect {
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
    state_machine_root_query: Query<&StateMachineRoot>,
    mut commands: Commands,
) {
    let target = trigger.target();
    let Ok(listener) = listener_query.get(target) else {
        return;
    };

    let root_entity = find_state_machine_root(target, &child_of_query, &state_machine_root_query);

    let Some(root_entity) = root_entity else {
        return;
    };

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