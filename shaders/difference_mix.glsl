// @category Mixing
// @role mix
// Marked difference of two inputs: shows only what changed between A and B,
// amplified. Abs is the plain |A-B|; Signed paints B-brighter in warm and
// B-darker in cool; Overlay lays the highlighted difference on top of A;
// Outline draws just the edges of the difference mask. Threshold ignores
// small noise, Gain makes the result punchy.
Properties {
    _Mode      ("Mode", Enum(Abs, Signed, Overlay, Outline)) = 0
    _Gain      ("Gain", Range(0, 12)) = 4.0
    _Threshold ("Threshold", Range(0, 1)) = 0.05
    _Softness  ("Softness", Range(0, 0.5)) = 0.05
    _Hue       ("Hue", Range(0, 1)) = 0.9
    _Amount    ("Amount", Range(0, 1)) = 1.0
}

vec3 hue(float h) {
    return clamp(abs(fract(h + vec3(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0) - 1.0, 0.0, 1.0);
}

float diffAt(vec2 uv) {
    vec3 a = texture(iChannel0, uv).rgb;
    vec3 b = texture(iChannel1, uv).rgb;
    float d = length(a - b) * 0.577;
    return smoothstep(_Threshold, _Threshold + _Softness + 1e-4, d);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec3 a = texture(iChannel0, uv).rgb;
    vec3 b = texture(iChannel1, uv).rgb;
    float d = clamp(diffAt(uv) * _Gain, 0.0, 1.0);
    vec3 col;
    if (_Mode == 1) {
        float s = dot(b - a, vec3(0.299, 0.587, 0.114));
        vec3 warm = vec3(1.0, 0.5, 0.1);
        vec3 cool = vec3(0.1, 0.5, 1.0);
        col = (s > 0.0 ? warm : cool) * d;
    } else if (_Mode == 2) {
        col = mix(a, a * 0.35 + hue(_Hue) * d * 1.2, d);
    } else if (_Mode == 3) {
        vec2 px = 1.0 / iResolution.xy;
        float e = abs(diffAt(uv + vec2(px.x * 2.0, 0.0)) - diffAt(uv - vec2(px.x * 2.0, 0.0)))
                + abs(diffAt(uv + vec2(0.0, px.y * 2.0)) - diffAt(uv - vec2(0.0, px.y * 2.0)));
        col = hue(_Hue) * clamp(e * 2.0, 0.0, 1.0) + a * 0.15;
    } else {
        col = abs(a - b) * _Gain;
    }
    fragColor = vec4(clamp(mix(a, col, _Amount), 0.0, 1.0), 1.0);
}
