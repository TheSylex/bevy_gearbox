# `state_machine!` Macro Enhancement Plan

This document outlines the plan to evolve the `state_machine!` macro into a more powerful, declarative, and safe state graph definition tool.

## 1. Core Goals

- **Declarative Syntax**: The macro should allow users to define a state graph, including allowed transitions and event-driven observers, in a single, clear definition.
- **Developer-Time Safety**: Invalid transitions should be caught at runtime with clear error messages in debug builds.
- **Leverage Bevy Idioms**: The implementation should build directly on Bevy's core features, especially entity-specific observers, for maximum performance and compatibility.
- **Ergonomics**: The syntax should be clean, non-redundant, and easy to reason about.

## 2. Proposed Syntax

The new syntax will be structured as a graph definition, where each state can define its outgoing transitions and its active observers.

```rust
state_machine!(Character => {
    // A state can have multiple definitions, grouped by braces.
    FreeMoveState => {
        // Defines an allowed transition to InChildSMState.
        // This will be used for runtime validation.
        InChildSMState,

        // Another allowed transition.
        DeadState,

        // An observer that is ONLY active when the entity is in FreeMoveState.
        |trigger: Trigger<TakeDamage>, query: Query<&Health>, mut commands: Commands| {
            if let Ok(health) = query.get(trigger.target()) {
                if health.value <= 0 {
                    commands.entity(trigger.target()).transition(DeadState);
                }
            }
        }
    },

    // A state can have a single, simple transition.
    RestingState => FreeMoveState,

    // A state can also have a single observer. The macro will infer the
    // event type from the closure's signature.
    InChildSMState => |trigger: Trigger<AbilityDone>, mut commands: Commands| {
        commands.entity(trigger.target()).transition(FinishedChildSMState);
    },

    FinishedChildSMState => FreeMoveState,

    // A terminal state has no transitions defined.
    DeadState,
});
```

## 3. High-Level Implementation

The `state_machine!` macro will be re-written to parse the new graph-like syntax and generate the necessary Bevy components and systems.

- **State Graph Parsing**: The macro will understand the `State => { ... }` definitions, including parsing transitions to other states and observer closures.

- **Observer Generation**: For each state with a closure `|trigger: Trigger<Event>, ...|`, the macro will generate systems to:
    - **Add** an entity-specific observer for `Event` when the entity enters the state.
    - **Remove** that observer when the entity exits the state.
    > **Note:** This implementation creates **per-state observers**. We may need to consider a syntax for **per-state-machine observers** in the future.

- **Transition Logic**: The existing state-exclusivity logic will be preserved. The list of allowed transitions (e.g., `StateA => StateB`) will be used to generate runtime validation checks to prevent invalid state changes.

- **Plugin Generation**: A Bevy plugin will be created to register all necessary components and systems automatically.
