## Goal

Bring `bevy_gearbox` into semantic alignment with XState while preserving Bevy-friendly ergonomics and performance. This plan is phased so we can land changes incrementally with tests and migration notes.

## Non-goals (for now)

- Full SCXML import/export. Focus is on XState runtime semantics and API parity.
- Visual editor. Ensure the core engine is correct first.

## Invariants to uphold

- A machine’s active configuration is a set of atomic (leaf) states plus their ancestors up to (and including) the machine root.
- Transitions use LCA-based exit→enter order: exit child→parent, then enter parent→child.
- Eventless (“always”, “after”, raised) transitions should resolve in the same frame via observer chaining; an optional per-frame internal queue may be used for reliability. Strict “stable configuration” guarantees are out of scope.
- Default self-transitions are external (exit and re-enter), unless explicitly marked internal.

## Phase 1 — Eventless transitions (observer chaining + optional per-frame queue)

- Keep observer chaining as the primary mechanism: `EnterState` → `Always` → `Transition`.
- Add an optional, per-frame internal event queue on the machine root for reliability:
  - Queue raised/internal events and due "after" timers for the current frame only; process them before scanning new external events.
  - Apply a small per-frame step cap (e.g., 64–128) to guard against cycles; log a warning if exceeded.
- Maintain explicit system/observer ordering so `EnterState` effects are visible to `Always`, and `Always` before `Transition`.
 - Note: the current observer chaining behavior is considered sufficient; strict “stable configuration” microstep guarantees are out of scope.

Deliverables
- Edits: lightweight per-frame queue and step cap; timer integration for "after"; document observer ordering.
- Tests: linear Always chains resolve same frame; raised/after events handled in-frame; cycle cap engages.

## Phase 2 — Guards and machine context (ECS-first, no evaluator world access)

- Keep the current ECS-friendly `Guards` model (systems update a `Guards` component/entity).
- Add explicit composition semantics per transition (no central evaluator):
  - Default `AllOf` (current behavior): proceed only if the referenced `Guards` entity is clear (empty).
  - `AnyOf`: allow specifying multiple guard entities on a connection; proceed if at least one of them is clear.
  - `Not`: model via a specific guard system that asserts failure when a predicate is true, or via separate transitions; avoid generic world-reflective evaluators.
- Optional machine `Context` can be provided by users but is not required by the engine.

- Compositional guard logic tree (evaluated against the existing failure `HashSet`):
  - Store a `Logic` expression on a guard entity; evaluate it to determine if the transition is allowed.
  - Shape:
    - `AllOf(Vec<Logic>)`, `AnyOf(Vec<Logic>)`, `Not(Box<Logic>)`, `Leaf(String)`
  - Semantics:
    - `Leaf(k)` is true iff `k` is NOT present in the guard failure set.
    - `AllOf`/`AnyOf`/`Not` use standard boolean logic with short-circuiting.
    - Edge cases: `AllOf([]) = true`, `AnyOf([]) = false`.
  - Fallback: if no `Logic` is attached, keep current behavior (transition allowed iff the failure set is empty).

Deliverables
- Edits: add `Logic` tree type and evaluator in `guards`; store/evaluate logic on transition entities alongside their `Guards` failure set; respect composition/logic when checking `Guards` during a transition.
- Macro: `guard_logic!` to build trees ergonomically (e.g., `guard_logic!(all![leaf("A"), any![leaf("B"), not(leaf("C"))]])`).
- Optional (feature): string builder `guard_logic_from_str("A & (B | !C)")` with a tiny parser; off by default.
- Tests: AND/OR/NOT tree evaluation, empty vector edge cases, multi-entity AnyOf, fallback-to-empty-set behavior.

## Phase 3 — Actions and ordering

- Support action types in XState order:
  - Exit actions (on states exited), transition actions, entry actions (on states entered).
- Implement built-ins:
  - `assign` (mutate context), `raise` (internal event), `send`/`sendTo` (external/child), `cancel`.
- Deterministic ordering within a macrostep: exit→transition→entry, topological by hierarchy.

Built-in actions (to be provided as reusable observers or helpers)
- assign: mutate machine context or root components. Runs synchronously; scoped to TransitionActions or entry/exit as authored.
- raise: enqueue an internal event on the machine (handled before scanning more eventless work in the same frame when using the optional queue).
- send / sendTo: post an external event to a target entity (child/parent/root) via Commands.trigger_targets.
- cancel: cancel a delayed `After` or an invocation/task associated with the current state or edge.

Deliverables
- Edits: action registry/executor; attach action lists to states and transitions; TransitionActions envelope available with { source, edge, target }.
- Tests: ordering, assign side-effects, raise/send semantics.

## Phase 4 — Final states and done events

- Add `Final` marker for states.
- Emit `done.state.<id>` when a region reaches its final; bubble `done` to parent; emit `done.invoke.<id>` for completed invocations; `done.state.machine` when the root completes.

Deliverables
- Edits: final-state detection in microsteps; done-event bubbling.
- Tests: region finals, parent onDone transitions, machine completion.

## Phase 5 — History semantics hardening

- Keep shallow/deep as implemented; ensure save on exit happens before exit actions; restore precedence over `InitialState` when present.
- Validate illegal combinations (e.g., deep history with no prior snapshot) have defined behavior.

Deliverables
- Edits: minor ordering checks.
- Tests: shallow vs deep restoration, mixed with parallel regions.

## Phase 6 — Invocations (services)

- Add `Invoke` with lifecycle: start on entry, pass `ctx`, receive events, cancel on exit.
- Support success (`done.invoke`) and error (`error.platform`) events.
- Backed by Bevy tasks or async executors; pluggable adapters.

Deliverables
- Edits: invocation manager; cancellation tokens; event routing.
- Tests: start/stop with state entry/exit, done/error propagation, cancellation.

## Phase 7 — Test suite and parity checks

- Golden tests mirroring XState examples: hierarchy, parallel, history, self/internal/external, eventless chains, finals, invokes, delays.
- Property tests: event commutativity where applicable; no duplicate `Active` for ancestors; stable configuration uniqueness.

## Phase 8 — Performance and instrumentation

- Benchmarks: microstep throughput, event latency, memory.
- Telemetry hooks: counters for exits/enters, microsteps, queue depth.

## Migration plan

- 0.x minor releases with feature flags:
  - Introduce guard composition via `Logic` on transition entities; keep existing simple-failure-set behavior as fallback.
  - Optional per-frame internal queue for raised/after events with a step cap.
  - Actions executor/registry with built-in actions.
- Publish `MIGRATION_GUIDE` with before/after snippets.

## File touchpoints (initial)

- `src/lib.rs`: transition ordering envelope, optional per-frame internal event queue.
- `src/transitions.rs`: transition entity components (`Source`, `Target`, `EdgeKind`, `After`, `Always`, `EventEdge<...>` on transitions), systems to scan `Transitions(Vec<Entity>)` on events/enter/exit.
- `src/history.rs`: minor ordering clarifications.
- `src/guards.rs`: add `Logic` tree type and evaluator; integrate with transition checks.
- `src/prelude.rs`: re-export new APIs.

## Acceptance criteria for alignment

- Root is `Active` while machine runs; ancestors active whenever a descendant is active.
- Self-transitions re-enter by default; internal flag prevents reentry.
- Eventless transitions resolve same-frame via observer chaining (optional per-frame queue available; strict stability not guaranteed).
- Guards can compose via `Logic`.
- Actions run in XState order; `assign` and `raise` work; `send`/`cancel` are available.
- Final states emit `done` events; invocations start/stop with states; delays are cancelable.

## Next steps (actionable)

1. Phase 1: Optional per-frame internal queue and ordering docs; tests for Always chains and step cap.
2. Phase 2: Guard composition `Logic` and `guard_logic!` macro (+ optional parser); integrate with transitions; tests.
3. Phase 3: Actions executor and built-ins (`assign`, `raise`, `send`/`sendTo`, `cancel`); tests and ordering validation.
4. Phase 4: Finals and done events; tests.
5. Phase 5: History ordering clarifications; tests.
6. Phase 6: Invocations lifecycle; tests.
7. Phase 7: Parity test suite; property tests.
8. Phase 8: Benchmarks and telemetry hooks.


