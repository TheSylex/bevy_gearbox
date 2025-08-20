use bevy::{prelude::*, reflect::Reflect};

use crate::{EnterState, ExitState, StateChildOf, StateMachine};

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct Active;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct Inactive;

pub(crate) fn add_active(
    trigger: Trigger<EnterState>,
    mut commands: Commands,
) {
    let target = trigger.target();
    commands.queue(move |world: &mut World| {
        let root = world.query::<&StateChildOf>().query(world).root_ancestor(target);
        let Some(StateMachine(state_machine)) = world.entity(root).get::<StateMachine>() else { return; };
        if state_machine.contains(&target) {
            world.entity_mut(target).remove::<Inactive>().insert(Active);
        }
    });
    println!();
}

pub(crate) fn add_inactive(
    trigger: Trigger<ExitState>,
    mut commands: Commands,
) {
    let target = trigger.target();
    commands.queue(move |world: &mut World| {
        let root = world.query::<&StateChildOf>().query(world).root_ancestor(target);
        let Some(StateMachine(state_machine)) = world.entity(root).get::<StateMachine>() else { return; };
        if !state_machine.contains(&target) {
            world.entity_mut(target).remove::<Active>().insert(Inactive);
        }
    });
}