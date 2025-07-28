# Bevy Gearbox

A hierarchical statechart library for Bevy that implements UML-style statecharts using Bevy's ECS and observer systems.

## Supported Features

- **Hierarchical States** - Nested state machines with parent-child relationships
- **Parallel States** - Multiple concurrent state regions  
- **History States** - Both shallow and deep history restoration
- **Guards** - Conditional transitions with runtime evaluation
- **Event-Driven Transitions** - Type-safe event handling via Bevy observers
- **Active State Tracking** - Automatic `Active`/`Inactive` component management
- **Component State Effects** - States can insert/remove components on the root entity
- **Event Propagation** - Broadcast events to all active states
- **Complex Transition Logic** - Dynamic target determination based on event data
- **Native Bevy Integration** - Built on Bevy's ECS, observers, and entity hierarchy

## Missing Features (Compared to XState)

- **Final States** - Terminal states that stop the machine (Is this just a state without transitions?)
- **Delayed Transitions** - Time-based automatic transitions (`after: 1000`)
- **Eventless Transitions** - Condition-only transitions that fire immediately (`always`)
- **Context/Extended State** - Built-in data management within the state machine
- **Invoked Actors/Services** - Spawning child processes, promises, or observables
- **Transient States** - Pass-through conditional routing states
- **Wildcard Transitions** - Catch-all handlers for unhandled events (`*`)
- **State Persistence** - Built-in save/restore functionality
- **Meta Data & Tags** - State categorization and metadata system
