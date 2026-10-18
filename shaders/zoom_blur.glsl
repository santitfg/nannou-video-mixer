// @category Blur & Glow
// @role effect
// Radial blur from a center point: Zoom streaks outward like a warp jump,
// Spin smears around the center.
Properties {
    _Mode   ("Mode", Enum(Zoom, Spin)) = 0
    _Amount ("Amount", Range(0, 1)) = 0.3
    _CX     ("Center X", Range(0, 1)) = 0.5
    _CY     ("Center Y", Range(0, 1)) = 0.5
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec2 center = vec2(_CX, _CY);
    vec2 d = uv - center;
    vec3 sum = vec3(0.0);
    for (int i = 0; i < 24; i++) {
        float k = float(i) / 23.0 - 0.5;
        vec2 p;
        if (_Mode == 1) {
            float a = k * _Amount * 0.6;
            float s = sin(a);
            float c = cos(a);
            p = center + vec2(d.x * c - d.y * s, d.x * s + d.y * c);
        } else {
            p = center + d * (1.0 - k * _Amount);
        }
        sum += texture(iChannel0, p).rgb;
    }
    fragColor = vec4(sum / 24.0, 1.0);
}
