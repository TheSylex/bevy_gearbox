use bevy::{prelude::*, state::state::FreelyMutableState};
use crate::{EnterState, StateMachine};

/// Bridge a Gearbox chart's EnterState events to Bevy `States`,
/// setting `NextState<S>` when a chart node carrying `S` is entered.
pub trait AppBevyStateBridgeExt {
    fn add_state_bridge<S>(&mut self) -> &mut Self
    where
        S: States + FreelyMutableState + Component + Clone + 'static;
}

impl AppBevyStateBridgeExt for App {
    fn add_state_bridge<S>(&mut self) -> &mut Self
    where
        S: States + FreelyMutableState + Component + Clone + 'static,
    {
        self.add_observer(bridge_chart_to_bevy_state::<S>)
    }
}

fn bridge_chart_to_bevy_state<S: States + FreelyMutableState + Component + Clone + 'static>(
    trigger: On<EnterState>,
    mut next: ResMut<NextState<S>>,
    state_q: Query<&S>,
) {
    let target = trigger.event().event_target();
    if let Ok(s) = state_q.get(target) {
        next.set(s.clone());
    }
}

/// Commands helper to emit a transition event to a specific chart root, located by a marker `M`.
pub trait GearboxCommandsExt {
    /// Emit an EntityEvent to the chart root identified by marker `M`.
    fn emit_to_chart<M, E>(&mut self, event: E)
    where
        M: Component + 'static,
        E: EntityEvent + Clone + Send + Sync + 'static,
        for<'a> <E as Event>::Trigger<'a>: Default;

    /// Build and emit an EntityEvent using the resolved chart root `Entity` for marker `M`.
    /// Usage: `commands.emit_to_chart::<AppState>(|root| MyEvent::new(root))`.
    fn emit_to_chart<M>(&mut self, make: impl BuildEntityEvent + Send + 'static)
    where
        M: Component + 'static;
}

impl<'w, 's> GearboxCommandsExt for Commands<'w, 's> {
    fn emit_to_chart<M, E>(&mut self, mut event: E)
    where
        M: Component + 'static,
        E: EntityEvent + Clone + Send + Sync + 'static,
        for<'a> <E as Event>::Trigger<'a>: Default,
    {
        self.queue(move |world: &mut World| {
            let mut q = world.query_filtered::<Entity, With<M>>();
            if let Ok(root) = q.single(world) {
                *event.event_target_mut() = root;
                world.commands().trigger(event);
            }
        });
    }

    fn emit_to_chart<M>(&mut self, make: impl BuildEntityEvent + Send + 'static)
    where
        M: Component + 'static,
    {
        self.queue(move |world: &mut World| {
            let mut q = world.query_filtered::<Entity, With<M>>();
            if let Ok(root) = q.single(world) {
                make.trigger_into_world(world, root);
            }
        });
    }
}

/// Helper trait to infer the event type from the closure and trigger it into the world.
pub trait BuildEntityEvent {
    fn trigger_into_world(self, world: &mut World, root: Entity);
}

impl<F, E> BuildEntityEvent for F
where
    F: FnOnce(Entity) -> E + Send + 'static,
    E: EntityEvent + Clone + Send + Sync + 'static,
    for<'a> <E as Event>::Trigger<'a>: Default,
{
    fn trigger_into_world(self, world: &mut World, root: Entity) {
        let event = self(root);
        world.commands().trigger(event);
    }
}