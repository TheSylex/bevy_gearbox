## Goal

Bring `bevy_gearbox` into semantic alignment with XState while preserving Bevy-friendly ergonomics and performance. This plan is phased so we can land changes incrementally with tests and migration notes.

## Non-goals (for now)

- Full SCXML import/export. Focus is on XState runtime semantics and API parity.
- Visual editor. Ensure the core engine is correct first.

## Invariants to uphold

- A machine’s active configuration is a set of atomic (leaf) states plus their ancestors up to (and including) the machine root.
- Transitions use LCA-based exit→enter order: exit child→parent, then enter parent→child.
- Event processing is atomic: eventless (“always”, “after”, raised) transitions are microstepped to a stable configuration before the next external event.
- Default self-transitions are external (exit and re-enter), unless explicitly marked internal.

## Phase 1 — Core activation semantics (root, ancestors, self-transitions)

- Ensure ancestors (including the machine root) become active when any descendant becomes active.
  - Update initialization path: when `CurrentState` is empty, enter the full path root→…→target before drilling into initial/history.
  - Never re-enter the root during regular transitions; it stays active for the machine lifetime.
- Make self-transitions external by default.
  - Add per-transition mode: `internal` | `external` (default external). Internal does not re-enter if target is the source or its descendant.
- Validation: compound states must have `InitialState` or `History` (unless `Parallel`).

Deliverables
- Edits: `src/lib.rs` (initialization path, `transition_observer`), `Connection` (flag), validation utilities.
- Tests: ancestor entry on init, root `Active`, default self-transition reentry, compound-without-initial errors.

## Phase 2 — Eventless transitions (observer chaining + optional per-frame queue)

- Keep observer chaining as the primary mechanism: `EnterState` → `Always` → `Transition`.
- Add an optional, per-frame internal event queue on the machine root for reliability:
  - Queue raised/internal events and due "after" timers for the current frame only; process them before scanning new external events.
  - Apply a small per-frame step cap (e.g., 64–128) to guard against cycles; log a warning if exceeded.
- Maintain explicit system/observer ordering so `EnterState` effects are visible to `Always`, and `Always` before `Transition`.
 - Note: the current observer chaining behavior is considered sufficient; strict “stable configuration” microstep guarantees are out of scope.

Deliverables
- Edits: lightweight per-frame queue and step cap; timer integration for "after"; document observer ordering.
- Tests: linear Always chains resolve same frame; raised/after events handled in-frame; cycle cap engages.

## Phase 3 — Guards and machine context (ECS-first, no evaluator world access)

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
- Edits: extend `Connection` with a composition policy and optional multiple guard entities; add `Logic` tree type and evaluator in `guards`; respect composition/logic when checking `Guards` during a transition.
- Macro: `guard_logic!` to build trees ergonomically (e.g., `guard_logic!(all![leaf("A"), any![leaf("B"), not(leaf("C"))]])`).
- Optional (feature): string builder `guard_logic_from_str("A & (B | !C)")` with a tiny parser; off by default.
- Tests: AND/OR/NOT tree evaluation, empty vector edge cases, multi-entity AnyOf, fallback-to-empty-set behavior.

## Phase 4 — Transition entities and listeners

- Represent transitions as first-class entities (edges), not children in the state hierarchy.
- State entities own a `Transitions(Vec<Entity>)` listing outbound transition entities; the Vec order is the priority (first match wins).
- Transition entity components:
  - `Source(Entity)`, `Target(Entity)`, `TransitionKind { External, Internal }` (default External), optional `Name/Tags`.
  - Guards: own `Guards` failure set and optional `Logic` tree directly on the transition entity.
  - Triggers: `Always` marker (no event), `After { duration }` (timer attached/started on `EnterState(source)`, canceled on `ExitState(source)`), and `TransitionListener<E>` attached to the transition entity.
- Event handling:
  - On event `E` at a source state, iterate that state’s `Transitions` Vec in order; pick the first transition that has `TransitionListener<E>` and whose guards/logic pass; fire the transition.
  - On `EnterState(source)`, evaluate `Always` transitions; start `After` timers. On `ExitState(source)`, cancel `After` timers.
- Root discovery: never cache root on transitions; find via `ChildOf` traversal as needed.

Deliverables
- Edits: new `transitions` module (components/systems), add `Transitions(Vec<Entity>)` to states; move listeners onto transition entities; integrate `After` and `Always` into this model.
- Migration: support both `Connection`-embedded paths and `TransitionRef(Entity)` during rollout; prefer transition entities in editor/runtime going forward.
- Tests: priority by Vec order, internal/external semantics, guards on transition entity, Always/After behavior, parallel region scoping.

## Phase 5 — Actions and ordering

- Support action types in XState order:
  - Exit actions (on states exited), transition actions, entry actions (on states entered).
- Implement built-ins:
  - `assign` (mutate context), `raise` (internal event), `send`/`sendTo` (external/child), `cancel`.
- Deterministic ordering within a macrostep: exit→transition→entry, topological by hierarchy.

Deliverables
- Edits: action registry/executor; attach action lists to states and transitions; internal event queue integration.
- Tests: ordering, assign side-effects, raise/send semantics.

## Phase 6 — Final states and done events

- Add `Final` marker for states.
- Emit `done.state.<id>` when a region reaches its final; bubble `done` to parent; emit `done.invoke.<id>` for completed invocations; `done.state.machine` when the root completes.

Deliverables
- Edits: final-state detection in microsteps; done-event bubbling.
- Tests: region finals, parent onDone transitions, machine completion.

## Phase 7 — History semantics hardening

- Keep shallow/deep as implemented; ensure save on exit happens before exit actions; restore precedence over `InitialState` when present.
- Validate illegal combinations (e.g., deep history with no prior snapshot) have defined behavior.

Deliverables
- Edits: minor ordering checks; diagnostics.
- Tests: shallow vs deep restoration, mixed with parallel regions.

## Phase 8 — Invocations (services)

- Add `Invoke` with lifecycle: start on entry, pass `ctx`, receive events, cancel on exit.
- Support success (`done.invoke`) and error (`error.platform`) events.
- Backed by Bevy tasks or async executors; pluggable adapters.

Deliverables
- Edits: invocation manager; cancellation tokens; event routing.
- Tests: start/stop with state entry/exit, done/error propagation, cancellation.

## Phase 9 — Delayed transitions (“after”)

- Add delayed transitions with cancellation on exit.
- Timer management per state instance; integrate with microstep queue when timers fire.

Deliverables
- Edits: timer scheduler; cancel on exit.
- Tests: delays respected, canceled on exit, ordering with raises.

## Phase 10 — Test suite and parity checks

- Golden tests mirroring XState examples: hierarchy, parallel, history, self/internal/external, eventless chains, finals, invokes, delays.
- Property tests: event commutativity where applicable; no duplicate `Active` for ancestors; stable configuration uniqueness.

## Phase 11 — Performance and instrumentation

- Benchmarks: microstep throughput, event latency, memory.
- Telemetry hooks: counters for exits/enters, microsteps, queue depth.

## Migration plan

- 0.x minor releases with feature flags:
  - Introduce root/ancestor activation and self-transition behavior behind a flag; flip default next release.
  - Augment `Guards` with composition and (optionally) multiple guard entities per connection; keep existing usage working.
  - Transition entities: add support for `Transitions(Vec<Entity>)` and transition-owned listeners/guards while maintaining `Connection` during migration.
  - Keep `Always`; document ordering and optional per-frame queue usage.
- Publish `MIGRATION_GUIDE` with before/after snippets.

## File touchpoints (initial)

- `src/lib.rs`: initialization path, `transition_observer`, optional per-frame internal event queue, validation.
- `src/transition_listener.rs`: internal event routing; external/internal flag support.
- `src/transitions.rs` (new): transition entity components (`Source`, `Target`, `TransitionKind`, `After`, `Always`, `TransitionListener<...>` on transitions), systems to scan `Transitions(Vec<Entity>)` on events/enter/exit.
- `src/history.rs`: minor ordering clarifications.
- `src/guards.rs`: replace with predicate-based guards and context; keep shim temporarily.
- `src/active.rs`: no change besides root activation on init.
- `src/prelude.rs`: re-export new APIs.

## Acceptance criteria for alignment

- Root is `Active` while machine runs; ancestors active whenever a descendant is active.
- Self-transitions re-enter by default; internal flag prevents reentry.
- Eventless transitions resolve within a tick to a stable configuration.
- Guards evaluate over `(context, event)` and can compose.
- Actions run in XState order; `assign` and `raise` work; `send`/`cancel` are available.
- Final states emit `done` events; invocations start/stop with states; delays are cancelable.
- Validation catches common configuration errors.

## Next steps (actionable)

1. Implement Phase 1 with tests; add a feature flag `xstate_semantics` defaulting to on.
2. Add optional per-frame internal queue and ordering docs (Phase 2) and migrate `Always` users in examples.
3. Introduce guards composition and logic trees (Phase 3).
4. Implement transition entities and per-source `Transitions(Vec<Entity>)` with listeners and timers (Phase 4).
5. Add actions executor and ordering envelope (Phase 5).
6. Add finals/done and history hardening (Phases 6–7) with tests.
7. Ship invocations and delays (Phases 8–9).
8. Build out the parity test suite and performance/telemetry (Phases 10–11).


