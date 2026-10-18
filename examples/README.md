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

The examples are compiled into the program (`build.rs` embeds every `.json`
in `boxes/` and `compositions/`), so they show up in the sidebar's
**Examples** tab and cannot be edited from the app: adding one always puts a
copy on the canvas. Add or change a file here and rebuild to update them.

- `boxes/` — one small patch per module and per shader category, each
  working on its own (generators stand in for a camera). Clicking one adds
  it to the canvas and plugs it into the Output if that has no input yet.
- `compositions/` — whole patches. Clicking one replaces the canvas.
  `single-camera`, `two-cameras-blend-50-50` and `camera-shader-lfo` use
  camera names from the author's machine; they load unselected elsewhere.
