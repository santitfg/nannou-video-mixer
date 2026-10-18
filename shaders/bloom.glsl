// @category Blur & Glow
// @role effect
// Glow: the brightest parts bleed light into their surroundings.
Properties {
    _Threshold ("Threshold", Range(0, 1)) = 0.6
    _Intensity ("Intensity", Range(0, 3)) = 1.2
    _Radius    ("Radius", Range(1, 16)) = 6.0
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec2 step = _Radius / 3.0 / iResolution.xy;
    vec3 src = texture(iChannel0, uv).rgb;
    vec3 glow = vec3(0.0);
    float total = 0.0;
    for (int x = -3; x <= 3; x++) {
        for (int y = -3; y <= 3; y++) {
            float w = exp(-float(x * x + y * y) / 6.0);
            vec3 c = texture(iChannel0, uv + vec2(float(x), float(y)) * step).rgb;
            glow += max(c - _Threshold, 0.0) * w;
            total += w;
        }
    }
    glow /= total;
    fragColor = vec4(clamp(src + glow * _Intensity * 3.0, 0.0, 1.0), 1.0);
}
