# MolWeaver

Molecular modeling MVP vertical slice using **winit**, **wgpu**, and **egui**.

## Running MolWeaver Locally

This section describes how to build and run **MolWeaver** on a local desktop environment.  
MolWeaver is a native GUI application written in Rust and based on **winit + wgpu + egui**.  
No external runtime (e.g. Python, JVM) is required.

---

### Prerequisites

#### Required
- 64-bit operating system (Windows, macOS, or Linux)
- A working GPU driver
  - wgpu uses **Vulkan (Linux/Windows)**, **DirectX 12 (Windows)**, or **Metal (macOS)**
- Rust toolchain (via `rustup`)

#### Optional
- Internet access (required only if dependencies are not vendored)

---

### Installing Rust

Rust is installed via **rustup**.

**Linux / macOS**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**Windows (PowerShell)**
```powershell
winget install Rustlang.Rustup
```

After installation, verify:
```bash
rustc --version
cargo --version
```

---

### OS-Specific Notes

#### Windows
- Windows 10 or later (64-bit)
- Updated GPU driver (NVIDIA / AMD / Intel)
- Visual Studio Build Tools (C++ build tools)
  - Usually prompted automatically by `rustup`

#### macOS
- macOS 12 or later recommended
- Xcode Command Line Tools:
```bash
xcode-select --install
```
- Apple Silicon and Intel are both supported

#### Linux
- X11 or Wayland environment
- GPU driver (Mesa or vendor driver)
- Required packages (Ubuntu example):
```bash
sudo apt install build-essential pkg-config libx11-dev libxcb1-dev
```

---

### Cloning the Repository

```bash
git clone https://github.com/h-nabata/MolWeaver.git
cd MolWeaver
```

If needed, switch branches:
```bash
git checkout main
```

---

### Building and Running

#### Basic launch
```bash
cargo run
```

On the first build, compilation may take several minutes due to shader compilation and GPU backend setup.

#### Running with an input file
If CLI arguments are enabled:
```bash
cargo run -- path/to/sample.xyz
```

or:
```bash
cargo run -- path/to/sample.pdb
```

If MolWeaver is configured to auto-load a sample file (e.g. `assets/sample.xyz`), no arguments are required.

---

### Controls (Default)

Typical controls include:
- **Left mouse drag**: rotate camera
- **Mouse wheel**: zoom
- **Click**: select atom
- **Keyboard**
  - `Ctrl/Cmd + Z`: Undo
  - `Ctrl/Cmd + Shift + Z` or `Y`: Redo

An **egui overlay** may display debug information such as:
- atom count
- selected atom ID
- frame time / FPS

---

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
