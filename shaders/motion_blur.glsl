// @category Blur & Glow
// @role effect
// Directional blur: smears the image along a line (Angle in turns, 0 =
// horizontal) over Length pixels.
Properties {
    _Length ("Length", Range(0, 120)) = 30.0
    _Angle  ("Angle", Range(0, 1)) = 0.0
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    float a = _Angle * 6.2831853;
    vec2 dir = vec2(cos(a), sin(a)) * _Length / iResolution.xy;
    vec3 sum = vec3(0.0);
    for (int i = 0; i < 25; i++) {
        float k = float(i) / 24.0 - 0.5;
        sum += texture(iChannel0, uv + dir * k).rgb;
    }
    fragColor = vec4(sum / 25.0, 1.0);
}
