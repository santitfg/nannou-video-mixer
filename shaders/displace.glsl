// @category Distortion
// @role effect
// Displaces iChannel0 using a map: iChannel1 if plugged (its brightness, or
// its red/green as an x/y vector), otherwise built-in animated noise.
Properties {
    _Mode   ("Map", Enum(Luma, Red-Green, Noise)) = 0
    _Amount ("Amount", Range(0, 0.3)) = 0.05
    _Angle  ("Angle", Range(0, 6.2832)) = 0.0
    _Scale  ("Noise scale", Range(0.5, 20)) = 4.0
    _Speed  ("Noise speed", Range(0, 5)) = 0.5
}

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float vnoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec2 offset;
    if (_Mode == 2 || iChannelResolution[1].x < 1.0) {
        vec2 p = uv * _Scale + iTime * _Speed;
        offset = vec2(vnoise(p), vnoise(p + vec2(31.7, 17.3))) - 0.5;
    } else if (_Mode == 1) {
        offset = texture(iChannel1, uv).rg - 0.5;
    } else {
        float l = dot(texture(iChannel1, uv).rgb, vec3(0.299, 0.587, 0.114)) - 0.5;
        offset = vec2(cos(_Angle), sin(_Angle)) * l;
    }
    fragColor = texture(iChannel0, uv + offset * _Amount * 2.0);
}
