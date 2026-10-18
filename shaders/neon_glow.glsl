// @category Blur & Glow
// @role effect
// Neon: detects edges, colors them by Hue and makes them glow. Mix keeps
// part of the original picture underneath.
Properties {
    _Hue      ("Hue", Range(0, 1)) = 0.85
    _Gain     ("Edge gain", Range(0, 10)) = 4.0
    _Radius   ("Glow radius", Range(0.5, 10)) = 3.0
    _Original ("Original", Range(0, 1)) = 0.2
    _Rainbow  ("Rainbow", Range(0, 1)) = 0.0
}

vec3 hue(float h) {
    return clamp(abs(fract(h + vec3(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0) - 1.0, 0.0, 1.0);
}

float lum(vec2 uv) {
    return dot(texture(iChannel0, uv).rgb, vec3(0.299, 0.587, 0.114));
}

float edge(vec2 uv, vec2 px) {
    float gx = lum(uv + vec2(px.x, 0.0)) - lum(uv - vec2(px.x, 0.0));
    float gy = lum(uv + vec2(0.0, px.y)) - lum(uv - vec2(0.0, px.y));
    return length(vec2(gx, gy));
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec2 px = 1.0 / iResolution.xy;
    float e = 0.0;
    float total = 0.0;
    for (int x = -2; x <= 2; x++) {
        for (int y = -2; y <= 2; y++) {
            float w = exp(-float(x * x + y * y) / 4.0);
            e += edge(uv + vec2(float(x), float(y)) * px * _Radius, px) * w;
            total += w;
        }
    }
    e = clamp(e / total * _Gain, 0.0, 1.5);
    float h = _Hue + _Rainbow * (uv.x + uv.y + iTime * 0.1);
    vec3 col = hue(h) * e * 1.5 + vec3(e * e * 0.5);
    vec3 src = texture(iChannel0, uv).rgb;
    fragColor = vec4(clamp(col + src * _Original, 0.0, 1.0), 1.0);
}
