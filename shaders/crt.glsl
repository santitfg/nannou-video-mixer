// @category Stylize
// @role effect
// Old CRT monitor: curved glass, scanlines, phosphor mask, vignette and noise.
Properties {
    _Scanlines ("Scanlines", Range(0, 1)) = 0.5
    _Curvature ("Curvature", Range(0, 0.5)) = 0.15
    _Vignette  ("Vignette", Range(0, 1)) = 0.5
    _Noise     ("Noise", Range(0, 0.5)) = 0.05
}

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 c = fragCoord / iResolution.xy * 2.0 - 1.0;
    c *= 1.0 + _Curvature * dot(c, c) * 0.25;
    vec2 uv = c * 0.5 + 0.5;
    // Past the curved glass edge: black.
    vec2 inside = step(vec2(0.0), uv) * step(uv, vec2(1.0));
    vec3 col = texture(iChannel0, uv).rgb * inside.x * inside.y;
    float scan = 0.5 + 0.5 * sin(uv.y * iResolution.y * 3.14159);
    col *= 1.0 - _Scanlines * 0.6 * scan;
    // Phosphor triads: every third column favors red, green or blue.
    float column = floor(mod(fragCoord.x, 3.0));
    vec3 mask = vec3(0.7) + 0.3 * vec3(1.0 - step(0.5, column), step(0.5, column) * (1.0 - step(1.5, column)), step(1.5, column));
    col *= mix(vec3(1.0), mask, _Scanlines * 0.5);
    col *= mix(1.0, smoothstep(1.4, 0.4, length(c)), _Vignette);
    col += (hash(fragCoord + fract(iTime) * 100.0) - 0.5) * _Noise;
    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
