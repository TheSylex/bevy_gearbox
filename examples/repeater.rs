use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy_gearbox::prelude::*;
use bevy_gearbox::transitions::Source;
use bevy_gearbox::GearboxPlugin;
use std::time::Duration;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(GearboxPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, (input_system, repeater_system))
        .add_observer(transition_edge_listener::<CastAbility>)
        .add_observer(transition_edge_listener::<OnComplete>)
        .add_observer(print_enter_state_messages)
        .add_observer(reset_repeater_on_cast)
        .add_observer(propagate_event::<CastAbility>)
        .add_observer(propagate_event::<OnComplete>)
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
#[derive(Event, Clone)]
struct CastAbility;

/// An event fired by a state when its internal logic has completed.
#[derive(Event, Clone)]
struct OnComplete;

/// Creates the ability state machine hierarchy.
fn setup(mut commands: Commands) {
    // --- State Machine Hierarchy ---
    // First, create the child entities to get their IDs
    let ready = commands.spawn(Name::new("Ready")).id();
    let repeating = commands
        .spawn((
            Name::new("Repeating"),
            Repeater {
                timer: Timer::new(Duration::from_secs(1), TimerMode::Repeating),
                remaining: 5,
            },
        ))
        .id();

    // Now create the root entity with all components including InitialState
    let machine_entity = commands
        .spawn((
            AbilityMachine,
            InitialState(ready),
            CurrentState(HashSet::new()),
            Name::new("AbilityStateMachine"),
            StateMachineRoot,
        )).id();

    // Add the child entities to the root
    commands.entity(machine_entity).add_child(ready);
    commands.entity(machine_entity).add_child(repeating);

    // --- Define Transition Entities ---
    // Ready --(CastAbility)--> Repeating
    commands
        .spawn((
            Name::new("Ready -> Repeating (CastAbility)"),
            Target(repeating),
            TransitionListener::<CastAbility>::default(),
            Source(ready),
        ));

    // Repeating --(OnComplete)--> Ready
    commands
        .spawn((
            Name::new("Repeating -> Ready (OnComplete)"),
            Target(ready),
            TransitionListener::<OnComplete>::default(),
            Source(repeating),
        ));
}

/// Listens for keyboard input and sends events to trigger state transitions.
fn input_system(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    query: Query<Entity, With<AbilityMachine>>,
    mut commands: Commands
) {
    let machine = query.single().unwrap();
    // Press 'C' to cast or reset the ability.
    if keyboard_input.just_pressed(KeyCode::KeyC) {
        println!("\n--- 'C' Pressed: Sending CastAbility event! ---");
        commands.trigger_targets(CastAbility, machine);
    }
}

/// The core logic for the repeater. Ticks the timer and fires "projectiles".
fn repeater_system(
    mut repeater_query: Query<(Entity, &mut Repeater), With<Active>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    // This system only runs when the machine is in the `Repeating` state.
    for (entity, mut repeater) in repeater_query.iter_mut() {
        repeater.timer.tick(time.delta());
        if repeater.timer.just_finished() {
            if repeater.remaining > 0 {
                println!("   => PEW! ({} remaining)", repeater.remaining - 1);
                repeater.remaining -= 1;
            }

            if repeater.remaining == 0 {
                // The repeater is done. Fire the `OnComplete` event on the `Repeating`
                // state entity. The `TransitionListener` on that entity will handle
                // transitioning back to the `Ready` state.
                commands.trigger_targets(OnComplete, entity);
            }
        }
    }
}

/// When we re-enter the 'Ready' state, reset the repeater's values.
fn reset_repeater_on_cast(
    trigger: Trigger<ExitState>,
    mut repeater_query: Query<&mut Repeater>,
) {
    let target = trigger.target();
    if let Ok(mut repeater) = repeater_query.get_mut(target) {
        repeater.remaining = 5;
        repeater.timer.reset();
    }
}

/// A debug system to print a message every time any state is entered.
fn print_enter_state_messages(trigger: Trigger<EnterState>, query: Query<&Name>) {
    if let Ok(name) = query.get(trigger.target()) {
        println!("[STATE ENTERED]: {}", name);
    }
}