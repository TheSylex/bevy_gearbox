use bevy::prelude::*;
use bevy_gearbox::prelude::*;
use bevy_gearbox::GearboxPlugin;

// --- Events ---

#[derive(Event, Clone)]
struct OnHit { pub target: Entity }

#[derive(Event, Clone)]
struct ApplyDamage { pub target: Entity }

// Map the trigger (OnHit) into a phase payload that emits ApplyDamage on Entry
impl TransitionEvent for OnHit {
    type EntryEvent = ApplyDamage;

    fn to_entry_event(&self) -> Option<Self::EntryEvent> {
        Some(ApplyDamage { target: self.target })
    }
}

// --- State markers ---

#[derive(Component, Clone)]
struct Flying;

#[derive(Component, Clone)]
struct DealDamage;

#[derive(Component)]
struct ProjectileMachine;

#[derive(Component)]
struct DummyTarget;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(GearboxPlugin)
        .add_transition_event::<OnHit>()
        .add_observer(print_enter_state)
        .add_observer(apply_damage_system)
        .add_systems(Startup, setup)
        .add_systems(Update, input_fire_hit)
        .add_state_component::<Flying>()
        .add_state_component::<DealDamage>()
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    commands.queue(|world: &mut World| {
        // Root machine
        let machine = world.spawn((
            Name::new("ProjectileMachine"),
            ProjectileMachine,
            StateMachine::new(),
        )).id();

        // States
        let flying = world.spawn((
            Name::new("Flying"),
            StateChildOf(machine),
            StateComponent(Flying),
        )).id();

        let deal_damage = world.spawn((
            Name::new("DealDamage"),
            StateChildOf(machine),
            StateComponent(DealDamage),
        )).id();

        // Transition: Flying --(OnHit{target})--> DealDamage
        world.spawn((
            Name::new("OnHit -> DealDamage"),
            Source(flying),
            Target(deal_damage),
            EventEdge::<OnHit>::default(),
        ));

        // Initial state
        world.entity_mut(machine).insert(InitialState(flying));

        // A dummy target entity to aim at
        world.spawn((
            Name::new("DummyTargetEntity"),
            DummyTarget,
        ));
    });
}

// Press Space to trigger an OnHit targeted at the machine, carrying the hit target entity.
fn input_fire_hit(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    machines: Query<Entity, With<ProjectileMachine>>,
    dummy: Query<Entity, With<DummyTarget>>,
    mut commands: Commands,
) {
    if !keyboard_input.just_pressed(KeyCode::Space) { return; }
    let Ok(machine) = machines.single() else { return; };
    let Ok(target) = dummy.single() else { return; };

    println!("\n-- Space pressed: simulate hit -> OnHit{{target:{:?}}}", target);
    commands.trigger_targets(OnHit { target }, machine);
}

// Demonstrate that the Entry phase received ApplyDamage with the original target from OnHit
fn apply_damage_system(
    trigger: Trigger<ApplyDamage>,
    names: Query<&Name>,
) {
    let ApplyDamage { target } = trigger.event().clone();
    let target_name = names.get(target).ok().map(|n| n.as_str().to_string()).unwrap_or_else(|| format!("Entity#{:?}", target));
    println!("[ApplyDamage] Entry payload received with target: {}", target_name);
}

fn print_enter_state(trigger: Trigger<EnterState>, names: Query<&Name>) {
    if let Ok(name) = names.get(trigger.target()) {
        println!("[EnterState] {}", name);
    }
}