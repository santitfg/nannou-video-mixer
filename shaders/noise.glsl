// @category Generators
// @role generator
// Noise generator (no input): smooth value noise, fractal clouds, ridged
// mountains-like lines, turbulent marble, or plain TV static.
Properties {
    _Type     ("Type", Enum(Smooth, Fractal, Ridged, Marble, Static)) = 1
    _Scale    ("Scale", Range(0.5, 30)) = 4.0
    _Speed    ("Speed", Range(0, 5)) = 0.3
    _Octaves  ("Octaves", Range(1, 8)) = 5.0
    _Contrast ("Contrast", Range(0.2, 4)) = 1.0
    _Color    ("Color", Enum(Mono, Rainbow, Fire, Ice)) = 0
}

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float vnoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}

float fbm(vec2 p, float octaves, bool ridged) {
    float sum = 0.0;
    float amp = 0.5;
    float total = 0.0;
    for (int i = 0; i < 8; i++) {
        if (float(i) >= octaves) break;
        float n = vnoise(p);
        if (ridged) n = 1.0 - abs(n * 2.0 - 1.0);
        sum += n * amp;
        total += amp;
        p = p * 2.03 + vec2(17.1, 9.2);
        amp *= 0.5;
    }
    return sum / total;
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.y;
    vec2 p = uv * _Scale;
    float t = iTime * _Speed;
    float v;
    if (_Type == 0) {
        v = vnoise(p + t);
    } else if (_Type == 1) {
        v = fbm(p + vec2(t, t * 0.7), _Octaves, false);
    } else if (_Type == 2) {
        v = fbm(p + vec2(t * 0.5, -t), _Octaves, true);
        v = v * v;
    } else if (_Type == 3) {
        v = 0.5 + 0.5 * sin(p.x * 2.0 + fbm(p + t, _Octaves, false) * 12.0);
    } else {
        v = hash(fragCoord + fract(iTime * 13.7) * 1000.0);
    }
    v = clamp((v - 0.5) * _Contrast + 0.5, 0.0, 1.0);
    vec3 col = vec3(v);
    if (_Color == 1) {
        col = 0.5 + 0.5 * cos(6.2832 * (v + vec3(0.0, 0.33, 0.67)));
    } else if (_Color == 2) {
        col = clamp(vec3(v * 3.0, v * 3.0 - 1.0, v * 3.0 - 2.0), 0.0, 1.0);
    } else if (_Color == 3) {
        col = vec3(v * v, v * 0.9, 0.3 + v * 0.7);
    }
    fragColor = vec4(col, 1.0);
}
