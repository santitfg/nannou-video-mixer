// @category Glitch
// @role effect
// Splits the red and blue channels apart. Feed video into iChannel0.
Properties {
    _Amount ("Amount", Range(0, 0.1)) = 0.01
    _Angle  ("Angle", Range(0, 6.2832)) = 0.0
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec2 dir = vec2(cos(_Angle), sin(_Angle)) * _Amount;

    float r = texture(iChannel0, uv + dir).r;
    float g = texture(iChannel0, uv).g;
    float b = texture(iChannel0, uv - dir).b;
    fragColor = vec4(r, g, b, 1.0);
}
