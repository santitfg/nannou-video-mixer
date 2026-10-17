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
  Number input port on the module (labelled, with its live value). Plug a
  Number / LFO / Mult in to drive it; unplugged, it uses the default.
* `iChannel0..3` become Video input ports — only as many as the code
  mentions. The output resolution follows the first connected channel
  (640x360 when there are none).
* Available uniforms: `iResolution`, `iTime`, `iTimeDelta`, `iFrame`,
  `iFrameRate`, `iChannelResolution[]`, `iChannel0..3`. `iMouse` and
  `iDate` exist but are always zero.
* Types: `Float`, `Int`, `Range(min, max)`. No vectors/colors yet.

Notes: `texture(iChannelN, uv)` works, but `iChannelN` can't be passed to a
function as a `sampler2D` parameter. Only the output (or a module with its
preview on) is rendered, so shaders that aren't wired to anything cost nothing.
