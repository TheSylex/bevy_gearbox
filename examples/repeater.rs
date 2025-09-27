use bevy::prelude::*;
use bevy_gearbox::prelude::*;
use bevy_gearbox::transitions::Source;
use bevy_gearbox::GearboxPlugin;
//use bevy_inspector_egui::bevy_egui::EguiPlugin;
//use bevy_inspector_egui::quick::WorldInspectorPlugin;
use std::time::Duration;
use bevy_gearbox::StateChildOf;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(GearboxPlugin)
        //.add_plugins(EguiPlugin::default())
        //.add_plugins(WorldInspectorPlugin::new())
        .add_systems(Startup, setup)
        .add_systems(Update, (input_system, repeater_system))
        .add_transition_event::<CastAbility>()
        .add_transition_event::<OnComplete>()
        .add_observer(print_enter_state_messages)
        .add_observer(reset_repeater_on_cast)
        .run();
}

// --- State Machine Definition ---

/// The root of our ability's state machine.
#[derive(Component)]
struct AbilityMachine;

/// A component to manage the repeater's state.
#[derive(Component)]
struct Repeater {
    timer: Timer,
    remaining: u32,
}

// --- Event to trigger state transitions ---
#[derive(SimpleTransition, EntityEvent, Clone)]
struct CastAbility(Entity);

/// An event fired by a state when its internal logic has completed.
#[derive(SimpleTransition, EntityEvent, Clone)]
struct OnComplete(Entity);

/// Creates the ability state machine hierarchy.
fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    commands.queue(move |world: &mut World| {
        let machine_entity = world.spawn(()).id();
        let ready = world.spawn(()).id();
        let repeating = world.spawn(()).id();
        let ready_cast_ability = world.spawn(()).id();
        let repeating_on_complete = world.spawn(()).id();

        world.entity_mut(machine_entity).insert((
            Name::new("AbilityStateMachine"),
            AbilityMachine,
            StateMachine::new(),
            InitialState(ready),
        ));

        world.entity_mut(ready).insert((
            Name::new("Ready"),
            StateChildOf(machine_entity),
        ));

        world.entity_mut(repeating).insert((
            Name::new("Repeating"),
            StateChildOf(machine_entity),
            Repeater {
                timer: Timer::new(Duration::from_secs(1), TimerMode::Repeating),
                remaining: 5,
            },
        ));

        world.entity_mut(ready_cast_ability).insert((
            Name::new("Ready -> Repeating (CastAbility)"),
            Target(repeating),
            EventEdge::<CastAbility>::default(),
            EdgeKind::External,
            Source(ready),
        ));

        world.entity_mut(repeating_on_complete).insert((
            Name::new("Repeating -> Ready (OnComplete)"),
            Target(ready),
            EventEdge::<OnComplete>::default(),
            EdgeKind::External,
            Source(repeating),
        ));
    });
}

/// Listens for keyboard input and sends events to trigger state transitions.
fn input_system(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    q_ability_machine: Query<Entity, With<AbilityMachine>>,
    mut commands: Commands
) {
    let Ok(machine) = q_ability_machine.single() else { return };
    // Press 'C' to cast or reset the ability.
    if keyboard_input.just_pressed(KeyCode::KeyC) {
        println!("\n--- 'C' Pressed: Sending CastAbility event! ---");
        commands.trigger(CastAbility(machine));
    }
}

/// The core logic for the repeater. Ticks the timer and fires "projectiles".
fn repeater_system(
    mut q_repeater: Query<(Entity, &mut Repeater), With<Active>>,
    q_child_of: Query<&bevy_gearbox::StateChildOf>,
    time: Res<Time>,
    mut commands: Commands,
) {
    // This system only runs when the machine is in the `Repeating` state.
    for (entity, mut repeater) in q_repeater.iter_mut() {
        repeater.timer.tick(time.delta());
        if repeater.timer.just_finished() {
            if repeater.remaining > 0 {
                println!("   => PEW! ({} remaining)", repeater.remaining - 1);
                repeater.remaining -= 1;
            }

            let root_entity = q_child_of.root_ancestor(entity);

            if repeater.remaining == 0 {
                // The repeater is done. Fire the `OnComplete` event on the `Repeating`
                // state entity. The `EventEdge` on that entity will handle
                // transitioning back to the `Ready` state.
                commands.trigger(OnComplete(root_entity));
            }
        }
    }
}

/// When we re-enter the 'Ready' state, reset the repeater's values.
fn reset_repeater_on_cast(
    trigger: On<ExitState>,
    mut q_repeater: Query<&mut Repeater>,
) {
    let target = trigger.event().event_target();
    if let Ok(mut repeater) = q_repeater.get_mut(target) {
        repeater.remaining = 5;
        repeater.timer.reset();
    }
}

/// A debug system to print a message every time any state is entered.
fn print_enter_state_messages(trigger: On<EnterState>, query: Query<&Name>) {
    if let Ok(name) = query.get(trigger.event().event_target()) {
        println!("[STATE ENTERED]: {}", name);
    }
}