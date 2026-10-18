// @category Generators
// @role generator
// Ripples spreading from drifting drop points, like rain on water.
// The rings interfere with each other and shade as a lit surface.
Properties {
    _Speed ("Speed", Range(0, 5)) = 1.0
    _Drops ("Drops", Range(1, 8)) = 4.0
    _Freq  ("Ring frequency", Range(2, 60)) = 22.0
    _Decay ("Decay", Range(0.5, 8)) = 2.5
    _Tint  ("Tint", Range(0, 6.2832)) = 3.6
}

vec2 dropPos(float i, float t)
{
    float k = i * 7.13;
    return vec2(0.5 + 0.4 * sin(k + t * 0.37 + i), 0.5 + 0.4 * cos(k * 1.7 + t * 0.29));
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    float aspect = iResolution.x / iResolution.y;
    float t = iTime * _Speed;

    float h = 0.0;
    for (int i = 0; i < 8; i++) {
        float on = step(float(i), _Drops - 1.0);
        vec2 d = (uv - dropPos(float(i), t)) * vec2(aspect, 1.0);
        float r = length(d);
        float phase = fract(t * 0.25 + float(i) * 0.37);
        float front = phase * 1.2;
        float wave = sin((r - front) * _Freq) * exp(-abs(r - front) * _Decay * 4.0);
        h += on * wave * (1.0 - phase) * exp(-r * _Decay * 0.5);
    }

    float shade = 0.5 + 0.5 * clamp(h, -1.0, 1.0);
    vec3 deep = 0.5 + 0.5 * cos(_Tint + vec3(0.0, 1.0, 2.0));
    vec3 col = mix(deep * 0.15, deep, shade) + pow(shade, 6.0) * 0.4;
    fragColor = vec4(col, 1.0);
}
