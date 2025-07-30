use bevy::{prelude::*, reflect::Reflect};

use crate::{EnterState, ExitState};

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
    commands.entity(target).remove::<Inactive>().insert(Active);
}

pub(crate) fn add_inactive(
    trigger: Trigger<ExitState>,
    mut commands: Commands,
) {
    let target = trigger.target();
    commands.entity(target).remove::<Active>().insert(Inactive);
}