// @category Generators
// @role generator
// Gooey metaballs: blobs that orbit, stretch and merge into each other.
Properties {
    _Speed  ("Speed", Range(0, 5)) = 0.6
    _Balls  ("Balls", Range(2, 10)) = 6.0
    _Size   ("Size", Range(0.02, 0.4)) = 0.12
    _Thresh ("Threshold", Range(0.5, 3)) = 1.5
    _Hue    ("Hue shift", Range(0, 6.2832)) = 0.5
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = (fragCoord - 0.5 * iResolution.xy) / iResolution.y;
    float t = iTime * _Speed;

    float field = 0.0;
    for (int i = 0; i < 10; i++) {
        float on = step(float(i), _Balls - 1.0);
        float k = float(i) + 1.0;
        vec2 c = 0.35 * vec2(sin(t * (0.5 + 0.13 * k) + k * 2.1), cos(t * (0.4 + 0.11 * k) + k * 1.3));
        float d = length(uv - c);
        field += on * (_Size * _Size) / (d * d + 0.0005);
    }

    float inside = smoothstep(_Thresh - 0.15, _Thresh + 0.15, field);
    float rim = smoothstep(_Thresh * 0.55, _Thresh, field) - inside;
    vec3 body = 0.5 + 0.5 * cos(_Hue + field * 0.6 + vec3(0.0, 2.0, 4.0));
    vec3 col = body * inside + rim * vec3(1.0, 0.7, 0.9) * 0.6;
    fragColor = vec4(col, 1.0);
}
