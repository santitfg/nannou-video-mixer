// @category Generators
// @role generator
// Brownian motion texture: fractal noise (fBm) that drifts like smoke,
// with optional domain warping for a more liquid, organic flow.
Properties {
    _Speed  ("Speed", Range(0, 5)) = 0.4
    _Scale  ("Scale", Float) = 3.0
    _Octaves("Octaves", Range(1, 8)) = 5.0
    _Gain   ("Gain", Range(0.2, 0.8)) = 0.5
    _Warp   ("Warp", Range(0, 3)) = 1.0
    _Hue    ("Hue shift", Range(0, 6.2832)) = 0.0
}

float hash21(vec2 p)
{
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

float vnoise(vec2 p)
{
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    float a = hash21(i);
    float b = hash21(i + vec2(1.0, 0.0));
    float c = hash21(i + vec2(0.0, 1.0));
    float d = hash21(i + vec2(1.0, 1.0));
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

float fbm(vec2 p)
{
    float sum = 0.0;
    float amp = 0.5;
    float norm = 0.0;
    for (int i = 0; i < 8; i++) {
        float on = step(float(i), _Octaves - 1.0);
        sum += on * amp * vnoise(p);
        norm += on * amp;
        p = p * 2.03 + vec2(17.1, 9.2);
        amp *= _Gain;
    }
    return sum / max(norm, 0.0001);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec2 p = (uv - 0.5) * vec2(iResolution.x / iResolution.y, 1.0) * _Scale;
    float t = iTime * _Speed;

    vec2 q = vec2(fbm(p + vec2(0.0, t)), fbm(p + vec2(5.2, 1.3 - t)));
    float v = fbm(p + _Warp * (q - 0.5) * 2.0 + vec2(t * 0.5, 0.0));

    vec3 col = 0.5 + 0.5 * cos(6.2832 * (v + vec3(0.0, 0.33, 0.67)) + _Hue);
    fragColor = vec4(col * (0.35 + 0.8 * v), 1.0);
}
