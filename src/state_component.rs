use bevy::prelude::*;

use crate::{EnterState, ExitState};

/// A component that when added to a state entity, will insert the contained component
/// `T` into the state machine's root entity when this state is entered.
#[derive(Component)]
pub struct InsertRootWhileActive<T: Component>(pub T);

/// A component that when added to a state entity, will remove the component type `T`
/// from the state machine's root entity when this state is entered, and restore
/// the stored value when the state is exited.
#[derive(Component)]
pub struct RemoveRootWhileActive<T: Component + Clone>(pub T);

/// A generic system that adds a component `T` to the state machine's root entity
/// when a state with `InsertRootWhileActive<T>` is entered.
pub fn insert_root_while_enter<T: Component + Clone>(
    trigger: Trigger<EnterState>,
    query: Query<&InsertRootWhileActive<T>>,
    child_of_query: Query<&ChildOf>,
    mut commands: Commands,
) {
    let entered_state = trigger.target();
    let Ok(insert_component) = query.get(entered_state) else {
        return;
    };

    let root_entity = child_of_query.root_ancestor(entered_state);

    if root_entity != entered_state {
        commands.entity(root_entity).insert(insert_component.0.clone());
    }
}

/// A generic system that removes a component `T` from the state machine's root entity
/// when a state with `InsertRootWhileActive<T>` is exited.
pub fn insert_root_while_exit<T: Component>(
    trigger: Trigger<ExitState>,
    query: Query<&InsertRootWhileActive<T>>,
    child_of_query: Query<&ChildOf>,
    mut commands: Commands,
) {
    let exited_state = trigger.target();
    if !query.contains(exited_state) {
        return;
    };

    let root_entity = child_of_query.root_ancestor(exited_state);

    if root_entity != exited_state {
        commands.entity(root_entity).remove::<T>();
    }
}

/// A generic system that removes a component `T` from the state machine's root entity
/// when a state with `RemoveRootWhileActive<T>` is entered.
pub fn remove_root_while_enter<T: Component + Clone>(
    trigger: Trigger<EnterState>,
    query: Query<&RemoveRootWhileActive<T>>,
    child_of_query: Query<&ChildOf>,
    mut commands: Commands,
) {
    let entered_state = trigger.target();
    if !query.contains(entered_state) {
        return;
    };

    let root_entity = child_of_query.root_ancestor(entered_state);

    if root_entity != entered_state {
        commands.entity(root_entity).remove::<T>();
    }
}

/// A generic system that restores a component `T` to the state machine's root entity
/// when a state with `RemoveRootWhileActive<T>` is exited, using the stored clone.
pub fn remove_root_while_exit<T: Component + Clone>(
    trigger: Trigger<ExitState>,
    query: Query<&RemoveRootWhileActive<T>>,
    child_of_query: Query<&ChildOf>,
    mut commands: Commands,
) {
    let exited_state = trigger.target();
    let Ok(remove_component) = query.get(exited_state) else {
        return;
    };

    let root_entity = child_of_query.root_ancestor(exited_state);

    if root_entity != exited_state {
        commands.entity(root_entity).insert(remove_component.0.clone());
    }
}