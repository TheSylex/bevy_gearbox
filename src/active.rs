use bevy::prelude::*;

use crate::{EnterState, ExitState};


#[derive(Component)]
pub struct Active;

#[derive(Component)]
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