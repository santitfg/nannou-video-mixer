// @category Glitch
// @role effect
// Slice glitch: horizontal bands slide sideways with a hard cut, plus a
// vertical roll and VHS-style tracking noise. Every Rate the pattern
// changes; Hold keeps a pattern for a while.
Properties {
    _Amount  ("Amount", Range(0, 1)) = 0.5
    _Slices  ("Slices", Range(2, 60)) = 14.0
    _Rate    ("Rate", Range(0, 30)) = 10.0
    _Roll    ("Roll", Range(0, 1)) = 0.0
    _Tracking("Tracking", Range(0, 1)) = 0.3
    _Split   ("RGB split", Range(0, 0.05)) = 0.01
}

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    float t = floor(iTime * _Rate);
    float slice = floor(uv.y * _Slices);
    float r = hash(vec2(slice, t));
    float hit = step(1.0 - _Amount * 0.6, r);
    float shift = (hash(vec2(slice + 7.0, t)) - 0.5) * 0.4 * _Amount * hit;
    // Tracking: a band that wobbles near the bottom, like a worn tape.
    float band = smoothstep(0.85, 1.0, 1.0 - uv.y + sin(iTime * 2.0) * 0.05);
    shift += (hash(vec2(floor(uv.y * 200.0), t)) - 0.5) * 0.08 * _Tracking * band;
    vec2 p = vec2(uv.x + shift, uv.y + _Roll * fract(iTime * 0.3));
    p.y = fract(p.y);
    float s = _Split * (1.0 + hit * 3.0);
    vec3 col = vec3(
        texture(iChannel0, p + vec2(s, 0.0)).r,
        texture(iChannel0, p).g,
        texture(iChannel0, p - vec2(s, 0.0)).b);
    col += _Tracking * band * 0.15 * hash(fragCoord + t);
    fragColor = vec4(col, 1.0);
}
