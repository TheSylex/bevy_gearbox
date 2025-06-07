use std::marker::PhantomData;

use bevy::prelude::*;
use bevy_ecs::system::{EntityCommand, EntityCommands};


// Extended trait to include both transition and enter commands.
pub trait StateTransitionCommandsExt<T: Component> {
    fn transition(&mut self, state: T);
}

impl<'w, T: Component>
    StateTransitionCommandsExt<T>
    for EntityCommands<'w>
{
    fn transition(&mut self, state: T) {
        self.queue(TransitionStateCommand { state });
    }
}

#[derive(Event)]
pub struct Transition<T: Component>(pub T);

pub struct TransitionStateCommand<T: Component> {
    state: T,
}

impl<T: Component> EntityCommand for TransitionStateCommand<T> {
    fn apply(self, mut entity: EntityWorldMut) {
        entity.trigger(Transition(self.state));
    }
}














pub struct TryExitStateCommand<T, N> {
    new_state: N,
    _pd: PhantomData<T>,
}

impl<T: Component + Clone, N: Component + Clone> EntityCommand for TryExitStateCommand<T, N> {
    fn apply(self, mut entity: EntityWorldMut) {
        if let Some(current_state) = entity.get::<T>() {
            entity.trigger(OnExitState(current_state.clone()));
            entity.remove::<T>();
            entity.insert(self.new_state.clone());
            entity.trigger(OnEnterState(self.new_state.clone()));
        }
    }
}

pub trait StateExitCommandsExt<N: Component + Clone> {
    fn try_exit_state<T: Component + Clone>(&mut self, new_state: N);
}

impl<'w, N: Component + Clone>
    StateExitCommandsExt<N>
    for EntityCommands<'w>
{
    fn try_exit_state<T: Component + Clone>(&mut self, new_state: N) {
        self.queue(TryExitStateCommand { new_state, _pd: PhantomData::<T> });
    }
}

#[derive(Event)]
pub struct OnExitState<T: Component>(pub T);








#[derive(Event)]
pub struct OnEnterState<T: Component>(pub T);