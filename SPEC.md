# MolWeaver MVP Specification

## Executive Summary (MVP Boundary)
1. MolWeaver MVP is a desktop molecular viewer/editor with single-molecule focus.
2. The fixed GUI stack is winit + wgpu + egui; alternatives are not permitted for MVP.
3. The MVP supports only XYZ file loading from a fixed local file path.
4. Rendering uses instanced spheres with one shared mesh and per-atom instances.
5. Selection is single-atom only, with a visual highlight on selection.
6. Editing includes atom add/remove, bond add/remove, atom move, and undo/redo.
7. UI thread must never perform file I/O, heavy parsing, or full GPU buffer rebuilds.
8. Incremental GPU updates are required for camera, selection, and edits.
9. Performance targets are explicit and enforced by regression checks.
10. Implementation must not begin until this spec and ARCHITECTURE.md are finalized.

## 1. Purpose and Non-Goals
### Purpose
- Provide a minimal, responsive molecular viewer/editor to validate architecture and UX constraints.

### Non-Goals
- No chemistry validation (valence, aromaticity, constraints).
- No multi-molecule scene management.
- No bond order semantics beyond existence.
- No file export, undo history serialization, or advanced UI tooling.

## 2. Target Users and Core Use Cases
### Target Users
- Developers validating rendering and editing workflows.
- Researchers previewing small XYZ datasets.

### Top 3 MVP Use Cases
1. Load an XYZ file and inspect atom positions via orbit/zoom camera.
2. Select an atom and perform a single edit (move/add/remove) with undo/redo.
3. Add/remove a bond between two atoms and undo/redo the operation.

## 3. Target Platforms
- Windows 10+, macOS 13+, Linux (x86_64).
- Input devices: mouse + keyboard (trackpad supported as mouse-equivalent).

## 4. Target Molecular Scale and Performance Goals
### Scale
- MVP max size: 10,000 atoms, 20,000 bonds.
- Future upper bound (non-MVP): 100,000 atoms, 200,000 bonds.

### Performance Targets
| Metric | Target | Measurement Method |
| --- | --- | --- |
| Frame time | <= 16.7 ms (60 FPS) at 10k atoms | Release build, average over 10s |
| Interaction latency | <= 50 ms for camera updates | Timestamp input-to-present |
| Startup time | <= 2.0 s to first frame | Wall-clock from process start |

## 5. Initial File Format Support
- Supported in MVP: XYZ (single structure).
- Explicitly unsupported: PDB, SDF, MOL2, CIF.
- Extension policy: add formats only after defining parser complexity and worker-thread cost.

## 6. Mandatory MVP Features (with Priority)
### P0 (Must Ship)
- Visualization: orbit camera, zoom, instanced spheres.
- Selection: single-atom selection with highlight.
- Editing: atom add/remove, bond add/remove, atom move, undo/redo.

### P1 (Optional After P0)
- Basic HUD: FPS, atom count, file name.

## 7. Non-Functional Requirements (Critical)
### UI Thread Non-Blocking Rules
- Prohibited on UI thread: file I/O, heavy parsing, mesh generation, full buffer rebuilds.
- Allowed on UI thread: uniform updates, single-instance buffer writes, UI input handling.

### Incremental Update Policy
- Camera changes: update camera uniform buffer only.
- Selection changes: update one instance entry only.
- Edits: update only affected instance entries; no full instance buffer rewrite.

### Dependency Control Rules
- A dependency may be added only if it is required by fixed stack or reduces unsafe code.
- Rejection criteria: duplicates existing functionality, adds >10% build time, or adds unused features.
- Each dependency addition must document: purpose, alternatives considered, build/binary impact.

### Logging and Error Handling
- Minimum logging: load failures, command failures, GPU surface errors.
- Errors must be surfaced in UI status panel and never panic in release mode.

## 8. Acceptance Criteria
### CI Requirements
- `cargo fmt --all` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- `cargo test --all` passes.

### Manual Verification Checklist
- Window opens titled "MolWeaver".
- Orbit/zoom is responsive at 10k atoms.
- Single click selects one atom and highlights it.
- Add/remove atom/bond works and is undoable/redone.
- Camera or selection does not trigger full mesh rebuilds.

### Benchmarking and Regression Detection
- Maintain a baseline FPS benchmark at 10k atoms; fail CI if FPS < 55 on reference machine.
- Track startup time; fail CI if > 2.5 s on reference machine.

## 9. Open Decisions and Decision Rules
### GUI Framework Selection Criteria
- Must support winit-compatible event loop and overlay rendering.
- Rejection conditions: blocking UI thread, lacking GPU overlay support.

### Rendering Backend Evaluation Criteria
- Must support instancing with per-instance data updates.
- Rejection conditions: no uniform buffer support, no depth buffer.

### File I/O Strategy Evaluation Criteria
- Must support worker-thread parsing and non-blocking UI.
- Rejection conditions: synchronous I/O on UI thread, unbounded memory usage.

**Implementation must not begin until this spec and ARCHITECTURE.md are finalized.**
