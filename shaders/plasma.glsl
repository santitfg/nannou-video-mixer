// Generator: no inputs, just math. Plug Number/LFO modules into the
// ports named after the properties below.
Properties {
    _Speed ("Speed", Range(0, 10)) = 1.0
    _Scale ("Scale", Float) = 3.0
    _Hue   ("Hue shift", Range(0, 6.2832)) = 0.0
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec2 p = (uv - 0.5) * _Scale;
    float t = iTime * _Speed;

    float v = sin(p.x * 3.0 + t) + sin(p.y * 2.0 - t * 1.3)
            + sin((p.x + p.y) * 2.0 + t * 0.7)
            + sin(length(p) * 4.0 - t);

    vec3 col = 0.5 + 0.5 * cos(v + _Hue + vec3(0.0, 2.0, 4.0));
    fragColor = vec4(col, 1.0);
}
