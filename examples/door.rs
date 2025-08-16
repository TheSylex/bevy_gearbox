use bevy::prelude::*;
use bevy_gearbox::prelude::*;
use bevy_gearbox::transitions::{Source, After};
use bevy_gearbox::GearboxPlugin;
use bevy_inspector_egui::bevy_egui::EguiPlugin;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use std::time::Duration;
use bevy_gearbox::StateChildOf;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(GearboxPlugin)
        .add_plugins(EguiPlugin::default())
        .add_plugins(WorldInspectorPlugin::new())
        .add_systems(Startup, setup)
        .add_systems(Update, input_system)
        .add_observer(transition_listener::<PlayerNearby>)
        .add_observer(transition_listener::<PlayerNotNearby>)
        .add_observer(print_enter_state_messages)
        .add_observer(print_exit_state_messages)
        .add_observer(replay_deferred_events::<PlayerNearby>)
        .add_observer(replay_deferred_events::<PlayerNotNearby>)
        .run();
}

// --- State Machine Definition ---

/// The root of our door's state machine.
#[derive(Component)]
struct DoorMachine;

// --- Events ---

/// Event triggered when player gets nearby (W key)
#[derive(Event, Clone)]
struct PlayerNearby;

/// Event triggered when player moves away (E key)
#[derive(Event, Clone)]
struct PlayerNotNearby;

/// Creates the door state machine hierarchy.
fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    commands.queue(move |world: &mut World| {
        // Create state entities - we need intermediate states to defer events
        let machine_entity = world.spawn(()).id();
        let closed = world.spawn(()).id();
        let opening = world.spawn(()).id();
        let open = world.spawn(()).id();
        let closing = world.spawn(()).id();

        // Create transition entities
        let closed_to_opening = world.spawn(()).id();
        let opening_to_open = world.spawn(()).id();
        let open_to_closing = world.spawn(()).id();
        let closing_to_closed = world.spawn(()).id();

        // Set up the machine root
        world.entity_mut(machine_entity).insert((
            Name::new("DoorStateMachine"),
            DoorMachine,
            StateMachine::new(),
            InitialState(closed),
        ));

        // Set up states
        world.entity_mut(closed).insert((
            Name::new("Closed"),
            StateChildOf(machine_entity),
        ));

        world.entity_mut(opening).insert((
            Name::new("Opening"),
            StateChildOf(machine_entity),
            DeferEvents::<PlayerNotNearby>::new(), // Defer PlayerNotNearby while opening
        ));

        world.entity_mut(open).insert((
            Name::new("Open"),
            StateChildOf(machine_entity),
        ));

        world.entity_mut(closing).insert((
            Name::new("Closing"),
            StateChildOf(machine_entity),
            DeferEvents::<PlayerNearby>::new(), // Defer PlayerNearby while closing
        ));

        // Set up transitions - immediate event-driven transitions, then After delays
        world.entity_mut(closed_to_opening).insert((
            Name::new("Closed -> Opening (PlayerNearby)"),
            Target(opening),
            TransitionListener::<PlayerNearby>::default(),
            TransitionKind::External,
            Source(closed),
        ));

        world.entity_mut(opening_to_open).insert((
            Name::new("Opening -> Open (After 1s)"),
            Target(open),
            Source(opening),
            After { duration: Duration::from_secs(1) }, // 1 second opening delay
        ));

        world.entity_mut(open_to_closing).insert((
            Name::new("Open -> Closing (PlayerNotNearby)"),
            Target(closing),
            TransitionListener::<PlayerNotNearby>::default(),
            TransitionKind::External,
            Source(open),
        ));

        world.entity_mut(closing_to_closed).insert((
            Name::new("Closing -> Closed (After 1s)"),
            Target(closed),
            Source(closing),
            After { duration: Duration::from_secs(1) }, // 1 second closing delay
        ));
    });
}

/// Handles keyboard input for player proximity events.
fn input_system(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    query: Query<Entity, With<DoorMachine>>,
    mut commands: Commands
) {
    let Ok(machine) = query.single() else { return };
    
    // Press 'W' to simulate player getting nearby
    if keyboard_input.just_pressed(KeyCode::KeyW) {
        println!("\n--- 'W' Pressed: Player approaches door (PlayerNearby event) ---");
        commands.trigger_targets(PlayerNearby, machine);
    }
    
    // Press 'E' to simulate player moving away
    if keyboard_input.just_pressed(KeyCode::KeyE) {
        println!("\n--- 'E' Pressed: Player moves away from door (PlayerNotNearby event) ---");
        commands.trigger_targets(PlayerNotNearby, machine);
    }
}



/// Debug system to print messages when states are entered.
fn print_enter_state_messages(trigger: Trigger<EnterState>, query: Query<&Name>) {
    if let Ok(name) = query.get(trigger.target()) {
        println!("[STATE ENTERED]: {}", name);
        
        // Add some visual feedback for door actions
        match name.as_str() {
            "Closed" => println!("   🚪 Door is closed and locked."),
            "Opening" => println!("   🔄 Door is opening... (using After transition)"),
            "Open" => println!("   🚪 Door is wide open!"),
            "Closing" => println!("   🔄 Door is closing... (using After transition)"),
            _ => {}
        }
    }
}

/// Debug system to print messages when states are exited.
fn print_exit_state_messages(trigger: Trigger<ExitState>, query: Query<&Name>) {
    if let Ok(name) = query.get(trigger.target()) {
        println!("[STATE EXITED]: {}", name);
    }
}
