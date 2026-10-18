// @category Blur & Glow
// @role effect
// Anamorphic streaks: bright parts stretch into long light bars (lens-flare
// look). Angle rotates the streak direction (turns).
Properties {
    _Threshold ("Threshold", Range(0, 1)) = 0.6
    _Intensity ("Intensity", Range(0, 4)) = 1.5
    _Length    ("Length", Range(0, 0.5)) = 0.15
    _Angle     ("Angle", Range(0, 1)) = 0.0
    _Tint      ("Tint", Range(0, 1)) = 0.0
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    float a = _Angle * 6.2831853;
    vec2 dir = vec2(cos(a), sin(a) * iResolution.x / iResolution.y) * _Length;
    vec3 src = texture(iChannel0, uv).rgb;
    vec3 streak = vec3(0.0);
    float total = 0.0;
    for (int i = -12; i <= 12; i++) {
        float k = float(i) / 12.0;
        float w = exp(-k * k * 3.0);
        vec3 c = texture(iChannel0, uv + dir * k).rgb;
        streak += max(c - _Threshold, 0.0) * w;
        total += w;
    }
    streak /= total;
    vec3 tint = mix(vec3(1.0), vec3(0.5, 0.75, 1.5), _Tint);
    fragColor = vec4(clamp(src + streak * tint * _Intensity * 4.0, 0.0, 1.0), 1.0);
}
