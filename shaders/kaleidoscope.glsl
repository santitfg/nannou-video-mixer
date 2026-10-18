// @category Distortion
// @role effect
// Mirrors iChannel0 around the center into N wedges.
Properties {
    _Segments ("Segments", Int) = 6
    _Rotation ("Rotation", Float) = 0.0
    _Zoom     ("Zoom", Range(0.2, 4)) = 1.0
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = (fragCoord - 0.5 * iResolution.xy) / iResolution.y;
    float a = atan(uv.y, uv.x) + _Rotation;
    float r = length(uv) / _Zoom;

    float wedge = 6.2831853 / float(max(_Segments, 1));
    a = abs(mod(a, wedge) - 0.5 * wedge);

    vec2 st = vec2(cos(a), sin(a)) * r;
    vec2 tex_uv = st * vec2(iResolution.y / iResolution.x, 1.0) + 0.5;
    fragColor = texture(iChannel0, tex_uv);
}
