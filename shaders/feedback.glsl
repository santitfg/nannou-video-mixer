// @category Feedback
// @role feedback
// Video feedback: the input mixed with this module's own previous frame
// (iFeedback), zoomed/rotated a little every frame so trails drift.
//   Classic    — the input over a fading copy of the last frame
//   Additive   — trails pile up (light painting)
//   Watercolor — trails bleed, diffuse and wander like wet pigment
//   Water      — trails ripple and refract like seen through water
//   Tunnel     — strong zoom + spin into the center
//   Smear      — trails get pushed along the image's edges (flow)
Properties {
    _Mode     ("Mode", Enum(Classic, Additive, Watercolor, Water, Tunnel, Smear)) = 0
    _Feedback ("Feedback", Range(0, 0.99)) = 0.85
    _Zoom     ("Zoom", Range(0.9, 1.1)) = 1.01
    _Rotate   ("Rotate", Range(-0.1, 0.1)) = 0.0
    _Distort  ("Distort", Range(0, 1)) = 0.3
}

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float vnoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    float a = hash(i);
    float b = hash(i + vec2(1.0, 0.0));
    float c = hash(i + vec2(0.0, 1.0));
    float d = hash(i + vec2(1.0, 1.0));
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

float luma(vec3 c) {
    return dot(c, vec3(0.299, 0.587, 0.114));
}

// Zoom/rotate `uv` around the center, keeping the aspect ratio.
vec2 transform(vec2 uv, float zoom, float angle) {
    float aspect = iResolution.x / iResolution.y;
    vec2 c = uv - 0.5;
    c.x *= aspect;
    float s = sin(angle);
    float k = cos(angle);
    c = mat2(k, -s, s, k) * c;
    c /= zoom;
    c.x /= aspect;
    return c + 0.5;
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec2 px = 1.0 / iResolution.xy;
    vec3 src = texture(iChannel0, uv).rgb;
    vec2 q = transform(uv, _Zoom, _Rotate);
    vec3 col;

    if (_Mode == 1) {
        vec3 prev = texture(iFeedback, q).rgb;
        col = min(src + prev * _Feedback, vec3(1.0));
    } else if (_Mode == 2) {
        // Wet pigment: wander along a slow noise field, diffuse (5 taps)
        // and only take new paint where the input differs from the canvas.
        float t = iTime * 0.15;
        vec2 n = vec2(vnoise(uv * 5.0 + t), vnoise(uv * 5.0 + vec2(17.3, 4.1) - t)) - 0.5;
        vec2 w = q + n * px * (2.0 + _Distort * 14.0);
        vec2 dx = vec2(px.x * 1.5, 0.0);
        vec2 dy = vec2(0.0, px.y * 1.5);
        vec3 prev = texture(iFeedback, w).rgb * 0.36
                  + (texture(iFeedback, w + dx).rgb + texture(iFeedback, w - dx).rgb
                   + texture(iFeedback, w + dy).rgb + texture(iFeedback, w - dy).rgb) * 0.16;
        float bleed = smoothstep(0.0, 0.3, length(src - prev));
        col = mix(prev, src, (1.0 - _Feedback) * (0.3 + 0.7 * bleed));
        col += (vnoise(fragCoord * 0.35) - 0.5) * 0.01; // paper grain
    } else if (_Mode == 3) {
        // Rings from the center plus a slow cross swell, refracting the trail.
        vec2 c = uv - 0.5;
        float d = length(c) + 1e-5;
        float strength = 0.2 + _Distort * 2.0;
        vec2 ripple = (c / d) * sin(d * 60.0 - iTime * 4.0) * 0.004 * strength
                    + vec2(sin(uv.y * 25.0 + iTime * 1.7), cos(uv.x * 23.0 + iTime * 1.3)) * 0.002 * strength;
        vec3 prev = texture(iFeedback, q + ripple).rgb * vec3(0.97, 0.99, 1.0);
        col = mix(src, prev, _Feedback);
    } else if (_Mode == 4) {
        vec3 prev = texture(iFeedback, transform(uv, _Zoom + 0.03 + _Distort * 0.1, _Rotate + 0.02)).rgb;
        col = max(src, prev * _Feedback);
    } else if (_Mode == 5) {
        // Flow along the input's luminance gradient.
        float l = luma(src);
        float gx = luma(texture(iChannel0, uv + vec2(px.x * 2.0, 0.0)).rgb) - l;
        float gy = luma(texture(iChannel0, uv + vec2(0.0, px.y * 2.0)).rgb) - l;
        vec2 flow = vec2(gx, gy) * px * (40.0 + _Distort * 400.0);
        vec3 prev = texture(iFeedback, q - flow).rgb;
        col = mix(src, prev, _Feedback);
    } else {
        vec3 prev = texture(iFeedback, q).rgb;
        col = mix(src, prev, _Feedback);
    }
    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
