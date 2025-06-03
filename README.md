# `bevy_gearbox`

`bevy_gearbox` is a Bevy plugin that provides a flexible and extensible hierarchical state machine (HSM) system. It's designed to simplify the management of complex entity states and their transitions, especially in scenarios involving nested or dependent state logic (e.g., character abilities, complex AI behaviors).

## Features

- **Hierarchical State Machines:** Define state machines that can parent other state machines, allowing for complex, nested state logic.
- **State Blocking:** `RestingState` supports "blockers," preventing transitions to a child state machine if its requirements are not met (e.g., an ability on cooldown).
- **Bevy Integration:** Leverages Bevy's ECS, observers, and triggers for efficient state management.

## Core Concepts

### State Machine Definition

1.  **Owner Component:** A Bevy `Component` that acts as the identifier for a state machine.
    ```rust
    use bevy::prelude::Component;

    #[derive(Component)]
    struct Character;
    ```

2.  **State Components:** Plain Bevy `Component`s that represent individual states. The first state listed in the `state_machine!` macro **must implement `Default`**. States can hold data!
    ```rust
    use bevy::prelude::Component;

    #[derive(Component, Clone, Debug, Default)]
    struct IdleState;

    #[derive(Component, Clone, Debug)]
    struct RunningState;

    #[derive(Component, Clone, Debug)]
    struct SpecialAbilityState {
        power_level: u32,
    }
    ```

3.  **`state_machine!` Macro:** Invoked to tie the owner component to its possible states. This macro generates:
    *   A `<OwnerName>StateEnum` (e.g., `CharacterStateEnum`) that reflects the current state, deriving `Default` (defaulting to its first variant).
    *   A Bevy `Plugin` (e.g., `CharacterPlugin`) to register necessary systems.
    *   Systems to manage state transitions and component additions/removals.
    ```rust
    use bevy::prelude::*;
    use macros::state_machine; // Or bevy_gearbox::state_machine if re-exported by bevy_gearbox lib.rs

    // Assuming IdleState, RunningState, etc., are defined as above
    // And RestingState, InChildSMState, etc. are available from bevy_gearbox::prelude

    state_machine!(Character;
        IdleState,      // First state; IdleState must implement Default
        RunningState,
        // ... other states like bevy_gearbox::prelude::RestingState,
        // bevy_gearbox::prelude::InChildSMState for hierarchy
    );
    ```

### Initialization

-   When you add the owner component (e.g., `Character`) to an entity, the generated plugin automatically (on the next `app.update()`):
    1.  Adds the corresponding `<OwnerName>StateEnum` (e.g., `CharacterStateEnum`), set to its default variant (which is the first state listed in the macro).
    2.  Adds the component for that first state by calling its `default()` method (e.g., `IdleState::default()`).

    ```rust
    // Spawning an entity with the Character state machine
    commands.spawn(Character);
    // After app.update(), the entity will have:
    // - Character component
    // - CharacterStateEnum::IdleState (if IdleState is first and enum derives Default)
    // - IdleState component (from IdleState::default())
    ```
-   You can also initialize an entity with a specific state if the default initialization is not desired:
    ```rust
    commands.spawn((
        Character,
        RunningState, // Explicitly start in RunningState
        // CharacterStateEnum::RunningState, // Optionally, if you need to be super explicit
    ));
    // After app.update(), the entity will have:
    // - Character component
    // - CharacterStateEnum::RunningState
    // - RunningState component
    ```
    Note: If you add an explicit state component at spawn, ensure its corresponding enum value is also set if your systems rely on the enum being immediately correct before the first `update`. The transition system usually handles this consistency.

### State Transitions

-   Use the `StateTransitionCommandsExt` trait (typically available via `bevy_gearbox::prelude::*`):
    ```rust
    use bevy_gearbox::prelude::StateTransitionCommandsExt;

    commands.entity(character_entity).transition(RunningState);
    ```
-   The macro-generated systems handle removing the old state component and enum value, and adding the new ones.

### Hierarchical State Machines (HSM)

-   **Parent SM:** A state machine that can delegate control to a child state machine.
-   **Child SM:** A state machine that is controlled by a parent.

**Key HSM States (from `bevy_gearbox::prelude`):**

-   `RestingState`: The default state for an inactive child SM. Supports 'blockers.' Blockers prevent a child state machine from being activated. An ability might have `ManaCost` and a `Cooldown` blocker.
-   `WorkingState`: The state a child SM enters when activated by its parent.
-   `InChildSMState(Entity)`: The state a parent SM enters when it passes control to the child SM (whose entity is the argument).
-   `FinishedChildSMState(Entity)`: The state a parent SM is transitioned to when its child SM returns to `RestingState`. You must define your own `FinishedChildSMState` return conditions.
-   `FizzledState`: The state a child SM enters if its parent exits `InChildSMState` prematurely.

**HSM Flow Example:**

1.  **Parent (Character) wants to use an Ability (Child SM):**
    ```rust
    // Character SM definition includes InChildSMState, FinishedChildSMState, etc.
    // Ability SM definition includes WorkingState, RestingState, FizzledState, etc.
    // Both IdleState and the first state of Ability SM (e.g., AbilityIdleState) must implement Default.

    let character_entity = commands.spawn(Character).id(); // Starts in IdleState by default
    let ability_entity = commands.spawn(Ability) // Starts in its default state
        .set_parent(character_entity) // Optional, for organization
        .id();

    // Character transitions to use the ability
    commands.entity(character_entity).transition(InChildSMState(ability_entity));
    ```

2.  **`bevy_gearbox` systems take over (after `app.update()`):**
    *   If `ability_entity`'s `RestingState` (assuming it was transitioned there or started there and is a prerequisite for `WorkingState`) is not blocked:
        *   `character_entity` now has `InChildSMState(ability_entity)` component and its `CharacterStateEnum` is updated. Any other state component defined in the `Character` `state_machine!` macro call will be removed and `OnEnter<InChildSMState>` and `OnExit<IdleState>` triggers will be fired.
        *   `ability_entity` is transitioned from its current state (e.g. `RestingState`) to `WorkingState`. `RestingState` is removed, `WorkingState` is inserted, and appropriate `OnEnter<T>, OnExit<T>` triggers are fired.
    *   If `ability_entity`'s `RestingState` *is* blocked:
        *   `character_entity` transition to `InChildSMState` is reverted by `GearboxPlugin` systems (e.g., back to `RestingState` or the state it was in before attempting transition to `InChildSMState`).

3.  **Ability (Child SM) completes its work:**
    ```rust
    // Inside the Ability's logic (e.g., an ability_system)
    // Typically, an ability finishes by transitioning to RestingState
    commands.entity(ability_entity).transition(RestingState::new()); // Or appropriate default if RestingState implements Default
    ```

4.  **`bevy_gearbox` systems detect child completion:**
    *   `ability_entity` now has `RestingState`.
    *   `character_entity` is automatically transitioned from `InChildSMState` to `FinishedChildSMState(ability_entity)`.

5.  **Parent (Character) handles the child's completion:**
    A system on the parent SM would typically observe `FinishedChildSMState` and transition the parent to an appropriate next state (e.g., back to `IdleState`).
    ```rust
    fn character_handle_ability_completion(
        mut commands: Commands,
        query: Query<Entity, (With<Character>, Added<FinishedChildSMState>)> // Use Added to react once
    ) {
        for character_entity in query.iter() {
            info!("Character finished with an ability");
            commands.entity(character_entity).transition(IdleState::default()); // Or other appropriate state
        }
    }
    ```

## Setup

1.  **Add `bevy_gearbox` to your `Cargo.toml`:**
    ```toml
    [dependencies]
    bevy = "0.xx" # Your Bevy version
    bevy_gearbox = { path = "path/to/bevy_gearbox" } # Or from crates.io if published
    # The `macros` crate is a local dependency of bevy_gearbox:
    # macros = { path = "./macros" } (This should be declared within bevy_gearbox's Cargo.toml)
    ```
    Your project only needs to depend on `bevy_gearbox`.

2.  **Add the `GearboxPlugin` and your generated SM plugins to your Bevy app:**
    ```rust
    use bevy::prelude::*;
    use bevy_gearbox::GearboxPlugin;
    // Assuming CharacterPlugin and AbilityPlugin are generated by your state_machine! calls
    // and exist in the same scope or are properly imported.

    // Placeholder for where your generated plugins would be (depends on your file structure)
    // pub use crate::character_module::CharacterPlugin;
    // pub use crate::ability_module::AbilityPlugin;

    fn main() {
        App::new()
            .add_plugins(DefaultPlugins)
            .add_plugins(GearboxPlugin)
            // Add your character/ability specific plugins generated by state_machine!
            // .add_plugins(CharacterPlugin) // Example
            // .add_plugins(AbilityPlugin)   // Example
            // ... other setup ...
            .run();
    }
    ```

3.  **Define your state machine components and use the `state_machine!` macro as shown in "Core Concepts".** Ensure the first state in each `state_machine!` definition implements `Default`.

## Example Usage

See `bevy_gearbox/examples/character_state_machine.rs` for a runnable demonstration of hierarchical state transitions between a `Character` and an `Ability`. Adapt it to ensure Default is implemented for initial states as per the latest macro changes.

## How it Works (Internals)

-   The `state_machine!` macro generates:
    -   An `enum <OwnerName>StateEnum` (deriving `Component`, `Clone`, `Debug`, `Default`, `Reflect`) to track the current state. The `#[default]` attribute is applied to its first variant.
    -   A system triggered on `OnAdd<OwnerName>` component:
        -   Inserts `<OwnerName>StateEnum::default()`.
        -   Inserts `<FirstStateComponent>::default()`.
    -   Observer systems for each state transition: When `commands.entity().transition(NewState)` is called, these observers:
        -   Update the `<OwnerName>StateEnum` to the new state's variant.
        -   Remove the old state component.
        -   Add the `NewState` component.
    -   A Bevy `Plugin` named `<OwnerName>Plugin` to register all these generated systems.
-   The `GearboxPlugin` contains core systems that manage the HSM logic:
    -   `set_child_working_system`: When a parent enters `InChildSMState`, this system attempts to transition the child to `WorkingState` (or reverts parent if child is blocked).
    -   `return_to_parent_sm_system`: When a child SM enters `RestingState`, this system finds its parent (if any, that is in `InChildSMState` pointing to this child) and transitions the parent to `FinishedChildSMState`.
    -   `early_exit_child_state_trigger_system`: If a parent exits `InChildSMState` before the child has finished (e.g., parent is stunned), this system transitions the child to `FizzledState`.

## Future Considerations

-   More complex guard conditions for transitions.
-   Event-based transitions in addition to direct command transitions.
-   State history and "undo" capabilities.
-   Improved ergonomics for states requiring non-default initialization when they are the first state.

This README provides a comprehensive overview of `bevy_gearbox`. Let me know if you'd like any sections expanded or clarified! 