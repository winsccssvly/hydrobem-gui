# Roadmap (2026-11-11)

## State

The product is the Python wheel `hydrobem-gui`: the Python API, the GUI (Rust, egui + wgpu) and the
engine `hydrobem.dll` of HydroBEM 0.3.0 (C ABI 8), built by HydroBEM (`ARCHITECTURE.md`). The GUI
does what `REQUIREMENTS.md` describes; the examples are HydroBEM's scripts.

## Next

1. **Bring an edited model back to Python.** `hydrobem_gui.show(model)` opens it unsaved
   (`--untitled`); `wait=True` waits for the GUI but does not return what the user saved: return
   the saved path so `hb.Model.load(path)` brings it back.
2. The viewer: mesh-quality highlighting in the viewport (`mesh_quality`), animation of
   generalized and pressure modes (`mode_displacements`).
3. Physics the engine has and the GUI does not expose (`KNOWN_LIMITATIONS.md`), first roll and
   pitch of the body itself.
4. Continuous integration: the tests need `engine/hydrobem.dll`, which is not committed; a CI job
   must fetch a released DLL first.
