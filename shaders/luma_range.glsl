// @category Key
// @role effect
// Luma band key: keeps only the pixels whose brightness falls between Low
// and High (soft edges), or everything outside the band with Invert.
// Recolor tints the kept band instead of blacking out the rest.
Properties {
    _Mode     ("Mode", Enum(Keep band, Invert band, Matte)) = 0
    _Low      ("Low", Range(0, 1)) = 0.3
    _High     ("High", Range(0, 1)) = 0.7
    _Softness ("Softness", Range(0, 0.5)) = 0.05
    _Recolor  ("Recolor", Range(0, 1)) = 0.0
    _Hue      ("Recolor hue", Range(0, 1)) = 0.6
}

vec3 hue(float h) {
    return clamp(abs(fract(h + vec3(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0) - 1.0, 0.0, 1.0);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec3 c = texture(iChannel0, uv).rgb;
    float l = dot(c, vec3(0.299, 0.587, 0.114));
    float lo = min(_Low, _High);
    float hi = max(_Low, _High);
    float mask = smoothstep(lo - _Softness, lo + _Softness + 1e-4, l)
               * (1.0 - smoothstep(hi - _Softness, hi + _Softness + 1e-4, l));
    if (_Mode == 1) {
        mask = 1.0 - mask;
    }
    vec3 tinted = mix(c, hue(_Hue) * (0.3 + l), _Recolor);
    fragColor = vec4(_Mode == 2 ? vec3(mask) : tinted * mask, 1.0);
}
