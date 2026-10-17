// Gaussian blur (9x9 taps, spread by Radius pixels).
Properties {
    _Radius ("Radius", Range(0, 20)) = 4.0
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec2 step = _Radius / 4.0 / iResolution.xy;
    vec3 sum = vec3(0.0);
    float total = 0.0;
    for (int x = -4; x <= 4; x++) {
        for (int y = -4; y <= 4; y++) {
            float w = exp(-float(x * x + y * y) / 8.0);
            sum += texture(iChannel0, uv + vec2(float(x), float(y)) * step).rgb * w;
            total += w;
        }
    }
    fragColor = vec4(sum / total, 1.0);
}
