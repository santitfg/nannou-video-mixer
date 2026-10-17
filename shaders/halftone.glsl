// Print-style halftone dots: black on white, cyan/magenta/yellow screens, or
// full CMYK with a black (key) screen at 45 degrees.
Properties {
    _Mode  ("Mode", Enum(Mono, CMY, CMYK)) = 0
    _Size  ("Dot size", Range(3, 40)) = 8.0
    _Angle ("Angle", Range(0, 1.5708)) = 0.785
}

// The dot cell `fragCoord` falls in on a grid rotated by `angle`: returns
// the cell center (screen space) and the distance to it (in cells).
vec2 dotCell(vec2 fragCoord, float angle, float size, out float dist) {
    float s = sin(angle);
    float c = cos(angle);
    vec2 p = mat2(c, -s, s, c) * fragCoord;
    vec2 cell = (floor(p / size) + 0.5) * size;
    dist = length(p - cell) / size;
    return mat2(c, s, -s, c) * cell;
}

float ink(float coverage, float dist) {
    float r = sqrt(clamp(coverage, 0.0, 1.0)) * 0.7;
    return 1.0 - smoothstep(r - 0.08, r, dist);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    float size = max(_Size, 2.0);
    float d = 0.0;
    vec3 col = vec3(1.0);
    if (_Mode == 0) {
        vec2 center = dotCell(fragCoord, _Angle, size, d);
        vec3 c = texture(iChannel0, center / iResolution.xy).rgb;
        float l = dot(c, vec3(0.299, 0.587, 0.114));
        col = vec3(1.0 - ink(1.0 - l, d));
    } else if (_Mode == 2) {
        // Gray-component replacement: black ink takes the shared darkness,
        // the color screens print what's left.
        vec2 ck = dotCell(fragCoord, _Angle + 0.7854, size, d);
        vec3 sk = texture(iChannel0, ck / iResolution.xy).rgb;
        float inkK = ink(1.0 - max(sk.r, max(sk.g, sk.b)), d);
        vec2 cc = dotCell(fragCoord, _Angle + 0.2618, size, d);
        vec3 sc = texture(iChannel0, cc / iResolution.xy).rgb;
        float kc = 1.0 - max(sc.r, max(sc.g, sc.b));
        col.r -= ink((1.0 - sc.r - kc) / max(1.0 - kc, 1e-3), d);
        vec2 cm = dotCell(fragCoord, _Angle + 1.309, size, d);
        vec3 sm = texture(iChannel0, cm / iResolution.xy).rgb;
        float km = 1.0 - max(sm.r, max(sm.g, sm.b));
        col.g -= ink((1.0 - sm.g - km) / max(1.0 - km, 1e-3), d);
        vec2 cy = dotCell(fragCoord, _Angle, size, d);
        vec3 sy = texture(iChannel0, cy / iResolution.xy).rgb;
        float ky = 1.0 - max(sy.r, max(sy.g, sy.b));
        col.b -= ink((1.0 - sy.b - ky) / max(1.0 - ky, 1e-3), d);
        col *= 1.0 - inkK;
    } else {
        vec2 cc = dotCell(fragCoord, _Angle, size, d);
        col.r -= ink(1.0 - texture(iChannel0, cc / iResolution.xy).r, d);
        vec2 cm = dotCell(fragCoord, _Angle + 0.2618, size, d);
        col.g -= ink(1.0 - texture(iChannel0, cm / iResolution.xy).g, d);
        vec2 cy = dotCell(fragCoord, _Angle + 0.5236, size, d);
        col.b -= ink(1.0 - texture(iChannel0, cy / iResolution.xy).b, d);
    }
    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
