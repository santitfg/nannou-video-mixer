# Shaders

Every `.glsl` / `.frag` file in this folder shows up in the **Shader**
module's dropdown (Effects → Shader). The file is watched: save it and the
module recompiles live; on a compile error the last good version keeps
running and the error is shown inside the module.

## Format

A Unity-style `Properties` block (optional) followed by Shadertoy-style GLSL:

```glsl
Properties {
    _Speed ("Speed", Range(0, 10)) = 1.0   // clamped to the range
    _Scale ("Scale", Float) = 2.0
    _Steps ("Steps", Int) = 4              // exposed to GLSL as `int`
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    fragColor = texture(iChannel0, uv) * _Scale;
}
```

* Each property becomes a global uniform with that exact name **and** a
  number box on the module, next to its input port. Drag the box, click it
  to type a value, or right-click it to change its range / clamping. Plug a
  Number / LFO / Lag / Mult in to drive it: the cable's 0..1 is mapped onto
  the box's range.
* `iChannel0..3` become Video input ports — only as many as the code
  mentions. The output resolution follows the first connected channel
  (640x360 when there are none).
* `iFeedback` is this module's own previous frame (black on the first
  frame or after a resolution change). Sample it like a channel:
  `texture(iFeedback, uv)`. See `feedback.glsl`.
* Available uniforms: `iResolution`, `iTime`, `iTimeDelta`, `iFrame`,
  `iFrameRate`, `iChannelResolution[]`, `iChannel0..3`, `iFeedback`.
  `iMouse` and `iDate` exist but are always zero.
* Types: `Float`, `Int`, `Range(min, max)` and `Enum(A, B, ...)`. An Enum
  is an `int` in GLSL (0 = first option) and shows up as a dropdown of its
  options (a wired cable's 0..1 steps through them). No vectors/colors yet.
* `iChannelResolution[n].x` is 0 for an unplugged channel — handy to ignore
  it (see `mixer.glsl`).

```glsl
_Mode ("Mode", Enum(Classic, Additive, Water)) = 0
...
if (_Mode == 1) { ... }
```

## Favorites

In a Shader module's file dropdown, click the star at the end of a row to
pin that shader. Pinned shaders appear in the sidebar under **Effects** and
create a Shader with that file already loaded. Right-click one in the
sidebar (or click its star again) to unpin it.

## Limitations

* `iChannelN` / `iFeedback` can't be passed to a function as a
  `sampler2D` parameter: sample inside `mainImage` (helpers can take the
  sampled colors or coordinates instead).
* No early `return` inside `mainImage`: assign `fragColor` once at the end
  (use `step()`/`if` blocks to pick the value).

Only the output (or a module with its preview on) is rendered, so shaders
that aren't wired to anything cost nothing.
