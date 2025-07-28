use std::marker::PhantomData;
use bevy::prelude::*;
use crate::{Connection, Transition};


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