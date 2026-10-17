# Example compositions

These match the JSON produced by the **Save** button in the sidebar (see
`save_composition` / `SaveFile` in `src/main.rs`), and can be opened
directly with the **Load** button, which opens a native file picker.

## Format

```jsonc
{
  "output_id": 0,          // id of the (always-present, single) Output module
  "modules": [
    {
      "id": 0,
      "kind": "camera" | "blend" | "output" | "number" | "mult" | "lfo" | "shader",
      "pos": [x, y],        // canvas position, center-origin
      "minimized": false,
      "show_preview": false,
      "camera_name": "...", // camera modules only: must match a device's
                             // name exactly as it appears in that module's
                             // dropdown — adjust this to your own hardware
      "mix": 0.5,            // blend modules only: 0 = input 0, 1 = input 1
                             // (overridden while a Number is plugged into input 2)
      "shader_file": "..."   // shader modules only: file name in ./shaders
    }
  ],
  "connections": [
    { "from": 2, "to": 1, "to_input": 0 } // module 2's output -> module 1's input 0
  ]
}
```

## Files

- `single-camera.json` — one camera wired straight into Output.
- `two-cameras-blend-50-50.json` — two cameras ("Integrated Webcam HD" and
  "Razer Kiyo") each wired into one of a Blend module's two inputs, mix set
  to `0.5` (even split), Blend wired into Output.
