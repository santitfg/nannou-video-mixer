// @category Generators
// @role generator
// Marble / ink in water: domain-warped noise sliced into slow veins.
Properties {
    _Speed ("Speed", Range(0, 3)) = 0.25
    _Scale ("Scale", Float) = 2.5
    _Veins ("Veins", Range(1, 30)) = 8.0
    _Warp  ("Warp", Range(0, 4)) = 2.0
    _Hue   ("Hue shift", Range(0, 6.2832)) = 0.0
}

float hash21(vec2 p)
{
    p = fract(p * vec2(233.34, 851.73));
    p += dot(p, p + 23.45);
    return fract(p.x * p.y);
}

float vnoise(vec2 p)
{
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + vec2(1.0, 0.0)), f.x),
               mix(hash21(i + vec2(0.0, 1.0)), hash21(i + vec2(1.0, 1.0)), f.x), f.y);
}

float fbm(vec2 p)
{
    float s = 0.0;
    float a = 0.5;
    for (int i = 0; i < 5; i++) {
        s += a * vnoise(p);
        p = p * 2.0 + 3.7;
        a *= 0.5;
    }
    return s;
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec2 p = (uv - 0.5) * vec2(iResolution.x / iResolution.y, 1.0) * _Scale;
    float t = iTime * _Speed;

    vec2 w = vec2(fbm(p + t), fbm(p - t + 4.0));
    float n = fbm(p + _Warp * w);
    float vein = abs(sin(n * _Veins + t));
    float line = 1.0 - smoothstep(0.0, 0.35, vein);

    vec3 base = 0.5 + 0.5 * cos(_Hue + n * 3.0 + vec3(0.0, 2.0, 4.0));
    fragColor = vec4(mix(base * 0.35, base + 0.3, line), 1.0);
}
