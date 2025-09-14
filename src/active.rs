use bevy::{prelude::*, reflect::Reflect};

use crate::{EnterState, ExitState, StateChildOf, StateMachine};

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct Active;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct Inactive;

pub(crate) fn add_active(
    trigger: On<EnterState>,
    mut commands: Commands,
) {
    let target = trigger.event().event_target();
    commands.queue(move |world: &mut World| {
        let root = world.query::<&StateChildOf>().query(world).root_ancestor(target);
        let Some(state_machine) = world.entity(root).get::<StateMachine>() else { return; };
        if state_machine.active.contains(&target) {
            world.entity_mut(target).remove::<Inactive>().insert(Active);
        }
    });
}

pub(crate) fn add_inactive(
    trigger: On<ExitState>,
    mut commands: Commands,
) {
    let target = trigger.event().event_target();
    commands.queue(move |world: &mut World| {
        let root = world.query::<&StateChildOf>().query(world).root_ancestor(target);
        let Some(state_machine) = world.entity(root).get::<StateMachine>() else { return; };
        if !state_machine.active.contains(&target) {
            world.entity_mut(target).remove::<Active>().insert(Inactive);
        }
    });
}