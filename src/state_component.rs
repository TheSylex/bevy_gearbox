use bevy::{ecs::component::Mutable, prelude::*};

use crate::{EnterState, ExitState, StateChildOf};

/// A component that when added to a state entity, will insert the contained component
/// `T` into the state machine's root entity when this state is entered.
#[derive(Component)]
pub struct StateComponent<T: Component>(pub T);

/// A component that when added to a state entity, will remove the component type `T`
/// from the state machine's root entity when this state is entered, and restore
/// the stored value when the state is exited.
#[derive(Component)]
pub struct StateInactiveComponent<T: Component + Clone>(pub T);

/// A generic system that adds a component `T` to the state machine's root entity
/// when a state with `StateComponent<T>` is entered.
pub fn state_component_enter<T: Component<Mutability = Mutable> + Clone>(
    trigger: Trigger<EnterState>,
    query: Query<&StateComponent<T>>,
    child_of_query: Query<&StateChildOf>,
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
/// when a state with `StateComponent<T>` is exited.
pub fn state_component_exit<T: Component>(
    trigger: Trigger<ExitState>,
    query: Query<&StateComponent<T>>,
    child_of_query: Query<&StateChildOf>,
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
/// when a state with `StateInactiveComponent<T>` is entered.
pub fn state_inactive_component_enter<T: Component + Clone>(
    trigger: Trigger<EnterState>,
    query: Query<&StateInactiveComponent<T>>,
    child_of_query: Query<&StateChildOf>,
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
/// when a state with `StateInactiveComponent<T>` is exited, using the stored clone.
pub fn state_inactive_component_exit<T: Component + Clone>(
    trigger: Trigger<ExitState>,
    query: Query<&StateInactiveComponent<T>>,
    child_of_query: Query<&StateChildOf>,
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

/// Helper trait to add state component observers to an App.
pub trait StateComponentAppExt {
    /// Registers both enter and exit observers for `StateComponent<T>`.
    /// This is a convenience method to avoid having to register both observers manually.
    fn add_state_component<T: Component<Mutability = Mutable> + Clone>(&mut self) -> &mut Self;
    
    /// Registers both enter and exit observers for `StateInactiveComponent<T>`.
    /// This is a convenience method to avoid having to register both observers manually.
    fn add_state_inactive_component<T: Component<Mutability = Mutable> + Clone>(&mut self) -> &mut Self;
}

impl StateComponentAppExt for App {
    fn add_state_component<T: Component<Mutability = Mutable> + Clone>(&mut self) -> &mut Self {
        self.add_observer(state_component_enter::<T>)
            .add_observer(state_component_exit::<T>)
    }
    
    fn add_state_inactive_component<T: Component<Mutability = Mutable> + Clone>(&mut self) -> &mut Self {
        self.add_observer(state_inactive_component_enter::<T>)
            .add_observer(state_inactive_component_exit::<T>)
    }
}

/// Event to reset a subtree rooted at the target entity.
#[derive(Event, Reflect, Default)]
pub struct Reset;