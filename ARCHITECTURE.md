# MolWeaver MVP Architecture

## Architectural Invariants
- UI thread performs input, UI, and lightweight state updates only.
- All edits are commands with apply/undo; no direct mutation from UI handlers.
- AtomId/BondId are stable and never derived from Vec indices.
- Rendering uses one shared sphere mesh with instancing.
- Minor edits never trigger full instance buffer rebuilds.
- Implementation must not begin until this document and SPEC.md are finalized.

## 1. High-Level Architecture Overview
### Module Separation
- **app**: winit event loop, input handling, high-level orchestration.
- **ui**: egui overlay state and control intents.
- **render**: wgpu resources, pipelines, buffer updates, draw calls.
- **model**: molecule topology, coordinates, and command system.
- **io**: file loading/parsing on worker thread.

### Responsibilities
- app: dispatch events, schedule redraw, route intents to commands.
- ui: produce edit intents and status display only.
- render: manage GPU buffers and pipelines; no business logic.
- model: own molecule state and command history.
- io: parse XYZ into model structures asynchronously.

## 2. Data Model
### AtomId / BondId Strategy
- AtomId and BondId are monotonically increasing u64 identifiers.
- Direct Vec index access is forbidden in command logic.
- IDs must remain stable across edits and undo/redo.

### State Separation
- **Topology**: atom/bond graph and identifiers.
- **Coordinates**: per-atom positions.
- **Render cache**: GPU buffers and derived instance data.

### Selection and Visualization State
- Selection is stored in UI state (AtomId only).
- Visualization parameters (colors, highlights) derived from model state.

## 3. Editing Model
### Command Pattern
- All edits implement `apply(&mut Molecule)` and `undo(&mut Molecule)`.
- Commands store sufficient data for lossless undo/redo.

### Undo/Redo Rules
- Two stacks: undo and redo.
- Redo stack is cleared on new command execution.
- Stack capacity is bounded; oldest commands are dropped when full.

### Edit Invariants
- AtomId/BondId uniqueness preserved.
- Bond endpoints must exist after apply/undo.
- Undo followed by redo restores identical topology and coordinates.

## 4. Rendering Architecture (Conceptual)
### Shared Mesh + Instancing
- One vertex/index mesh for a unit sphere.
- Per-atom instance buffer with position, color, flags.

### Buffer Separation by Update Frequency
- **Static buffers**: sphere vertex/index.
- **Per-frame**: camera uniform.
- **Per-edit**: instance buffer entries.

### Prohibitions
- No full instance buffer rebuild for single-atom edits.
- No per-atom mesh generation.

## 5. Threading Model
### UI Thread
- winit event loop, egui overlay, command execution.
- GPU buffer updates must be incremental.

### Worker Thread
- File I/O and parsing only.
- Sends parsed Molecule via channel to UI thread.

### Synchronization Policy
- Message-based handoff; no shared mutable state across threads.

## 6. Testing and Measurement Strategy
### Unit Tests
- Command apply/undo for each edit type.
- XYZ parser success/failure cases.
- Element color mapping if present.

### Benchmarks
- Hot paths only: per-frame draw loop and per-edit buffer update.

## 7. Common Failure Modes and Mitigations
- **Full mesh rebuilds**: forbid by code review and clippy linting on buffer usage.
- **Excessive dependencies**: require documented rationale and impact analysis.
- **ID invalidation**: enforce stable IDs via API, avoid Vec index exposure.
- **UI blocking operations**: prohibit synchronous file I/O and heavy parsing on UI thread.

**Implementation must not begin until this document and SPEC.md are finalized.**
