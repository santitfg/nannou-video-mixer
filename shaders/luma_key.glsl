// @category Key
// @role effect
// Keeps the bright (or dark) parts and blacks out the rest; Matte shows the
// mask itself. Pair it with Blend in Add/Screen to layer the result.
Properties {
    _Mode      ("Mode", Enum(Keep bright, Keep dark, Matte)) = 0
    _Threshold ("Threshold", Range(0, 1)) = 0.3
    _Softness  ("Softness", Range(0, 0.5)) = 0.1
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec3 c = texture(iChannel0, uv).rgb;
    float l = dot(c, vec3(0.299, 0.587, 0.114));
    float mask = smoothstep(_Threshold - _Softness, _Threshold + _Softness + 1e-4, l);
    if (_Mode == 1) {
        mask = 1.0 - mask;
    }
    fragColor = vec4(_Mode == 2 ? vec3(mask) : c * mask, 1.0);
}
