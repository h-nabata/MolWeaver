# MolWeaver

Molecular modeling MVP vertical slice using **winit**, **wgpu**, and **egui**.

## Run

```bash
cargo run
```

The MVP loads a fixed sample XYZ file located at `assets/sample.xyz` on startup.
Edit or replace this file to try different molecules.

## Controls

- **Orbit**: Left mouse button drag
- **Zoom**: Mouse wheel
- **Pick**: Left click to select an atom (highlights)

## Editing

- **Tools**: Use the Edit panel to switch tools (Select / Add Atom / Add Bond / Move).
- **Representation**: Switch between Ball & Stick and Space Filling in the Edit panel.
- **Insert Atom**: Choose an element and click **Insert Atom**.
- **Bonds**: Select an atom, choose a bond target, then click **Add Bond** or **Remove Bond**.
- **Move Atom**: Select an atom, set a step, and use the axis buttons.
- **Undo/Redo**: Buttons in the Edit panel or keyboard shortcuts:
  - **Ctrl/Cmd + Z**: Undo
  - **Ctrl/Cmd + Shift + Z** or **Ctrl/Cmd + Y**: Redo

## Dependency notes

This MVP keeps dependencies minimal and strictly aligned with the fixed stack:

- **winit**: required for windowing and input events.
  - Alternatives considered: sdl2/glfw (rejected; not allowed by requirements).
  - Impact: moderate compile time, standard binary size for windowing.
- **wgpu**: required for GPU rendering of instanced spheres.
  - Alternatives considered: vulkano/glium (rejected; not allowed).
  - Impact: largest contributor to build time and binary size, but necessary for GPU rendering.
- **egui**, **egui-winit**, **egui-wgpu**: required for overlay UI only.
  - Alternatives considered: imgui-rs (rejected; not allowed).
  - Impact: small to moderate build-time increase; low runtime overhead for overlay usage.
- **glam**: lightweight math library for camera and ray calculations.
  - Alternatives considered: cgmath (similar), nalgebra (heavier).
  - Impact: small; simplifies vector/matrix math.
- **bytemuck**: required for safe zero-copy buffer uploads to wgpu.
  - Alternatives considered: manual `unsafe` transmute (rejected for safety).
  - Impact: negligible.
- **pollster**: minimal blocking helper to initialize wgpu async setup.
  - Alternatives considered: tokio/async-std (too heavy for MVP).
  - Impact: negligible.
