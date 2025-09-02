/// Manage app state with Gearbox! Why would I do this? 
/// 1. You can change or examine your app flow state using the gearbox editor
/// 2. Supports complex logic like nested substates, history, deferred events, and more!
/// 3. Hooks into bevy state management seemlessly

use bevy::prelude::*;
use bevy::state::state::FreelyMutableState;
use bevy_gearbox::prelude::*;
use bevy_gearbox::GearboxPlugin;

#[derive(States, Component, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
enum ExampleState {
    #[default]
    Menu,
    Playing,
    Paused,
}

#[derive(Event, Clone, bevy_gearbox::SimpleTransition)]
struct Start;

#[derive(Event, Clone, bevy_gearbox::SimpleTransition)]
struct Pause;

#[derive(Event, Clone, bevy_gearbox::SimpleTransition)]
struct Resume;

#[derive(Resource)]
struct AppState(Entity);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(GearboxPlugin)
        .init_state::<ExampleState>()
        .add_transition_event::<Start>()
        .add_transition_event::<Pause>()
        .add_transition_event::<Resume>()
        .add_systems(Startup, setup_machine)
        .add_observer(bridge_chart_to_app_state::<ExampleState>)
        .add_systems(OnEnter(ExampleState::Menu), || println!("ExampleState::Menu"))
        .add_systems(OnEnter(ExampleState::Playing), || println!("ExampleState::Playing"))
        .add_systems(OnEnter(ExampleState::Paused), || println!("ExampleState::Paused"))
        .add_systems(Update, demo_input)
        .run();
}

fn setup_machine(mut commands: Commands) {
    // Build a minimal chart:
    // root -> { menu, playing, paused }
    // menu --(Start)--> playing
    // playing --(Pause)--> paused
    // paused --(Resume)--> playing

    let root = commands.spawn(StateMachine::new()).id();

    let menu = commands.spawn((StateChildOf(root), ExampleState::Menu)).id();
    let playing = commands.spawn((StateChildOf(root), ExampleState::Playing)).id();
    let paused = commands.spawn((StateChildOf(root), ExampleState::Paused)).id();

    // Initial state is Menu
    commands.entity(root).insert(InitialState(menu));

    // Edges
    let _e_menu_start = commands
        .spawn((
            Source(menu),
            Target(playing),
            EventEdge::<Start>::default(),
        ))
        .id();

    let _e_play_pause = commands
        .spawn((
            Source(playing),
            Target(paused),
            EventEdge::<Pause>::default(),
        ))
        .id();

    let _e_pause_resume = commands
        .spawn((
            Source(paused),
            Target(playing),
            EventEdge::<Resume>::default(),
        ))
        .id();

    // Relationship macros on `Source`/`Transitions` will link edges to sources.

    commands.insert_resource(AppState(root));
}

fn bridge_chart_to_app_state<S: States + FreelyMutableState + Component + Clone>(
    trigger: Trigger<EnterState>,
    mut next: ResMut<NextState<S>>,
    state_q: Query<&S>,
) {
    if let Ok(s) = state_q.get(trigger.target()) {
        next.set(s.clone());
    }
}

fn demo_input(
    kb: Res<ButtonInput<KeyCode>>,
    chart_root: Res<AppState>,
    mut commands: Commands,
) {
    if kb.just_pressed(KeyCode::Digit1) {
        println!("Event: Start (Menu -> Playing)");
        commands.trigger_targets(Start, chart_root.0);
    }
    if kb.just_pressed(KeyCode::Digit2) {
        println!("Event: Pause (Playing -> Paused)");
        commands.trigger_targets(Pause, chart_root.0);
    }
    if kb.just_pressed(KeyCode::Digit3) {
        println!("Event: Resume (Paused -> Playing)");
        commands.trigger_targets(Resume, chart_root.0);
    }
}


