// Twists the image around the center; strongest in the middle.
Properties {
    _Angle  ("Angle", Range(-10, 10)) = 3.0
    _Radius ("Radius", Range(0.05, 1)) = 0.5
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 c = fragCoord - 0.5 * iResolution.xy;
    float r = length(c) / iResolution.y;
    float k = max(0.0, 1.0 - r / _Radius);
    float a = _Angle * k * k;
    float s = sin(a);
    float co = cos(a);
    c = mat2(co, -s, s, co) * c;
    fragColor = texture(iChannel0, (c + 0.5 * iResolution.xy) / iResolution.xy);
}
