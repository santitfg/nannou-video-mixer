// Zoom, rotate and move the image. Outside the edges: black, repeat or mirror.
Properties {
    _Zoom    ("Zoom", Range(0.1, 5)) = 1.0
    _Rotate  ("Rotate", Range(-3.1416, 3.1416)) = 0.0
    _OffsetX ("Offset X", Range(-1, 1)) = 0.0
    _OffsetY ("Offset Y", Range(-1, 1)) = 0.0
    _Wrap    ("Edges", Enum(Black, Repeat, Mirror)) = 0
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    float aspect = iResolution.x / iResolution.y;
    vec2 c = fragCoord / iResolution.xy - 0.5 - vec2(_OffsetX, _OffsetY) * 0.5;
    c.x *= aspect;
    float s = sin(_Rotate);
    float k = cos(_Rotate);
    c = mat2(k, -s, s, k) * c / max(_Zoom, 0.01);
    c.x /= aspect;
    vec2 uv = c + 0.5;
    float visible = 1.0;
    if (_Wrap == 1) {
        uv = fract(uv);
    } else if (_Wrap == 2) {
        uv = 1.0 - abs(mod(uv, 2.0) - 1.0);
    } else {
        vec2 inside = step(vec2(0.0), uv) * step(uv, vec2(1.0));
        visible = inside.x * inside.y;
    }
    fragColor = vec4(texture(iChannel0, uv).rgb * visible, 1.0);
}
