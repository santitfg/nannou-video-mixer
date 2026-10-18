// @category Distortion
// @role effect
// Sine-wave displacement: rows sway, columns sway, or rings spread out.
Properties {
    _Mode      ("Mode", Enum(Horizontal, Vertical, Radial)) = 0
    _Amplitude ("Amplitude", Range(0, 0.1)) = 0.02
    _Frequency ("Frequency", Range(1, 60)) = 10.0
    _Speed     ("Speed", Range(0, 10)) = 2.0
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    float t = iTime * _Speed;
    if (_Mode == 0) {
        uv.x += sin(uv.y * _Frequency + t) * _Amplitude;
    } else if (_Mode == 1) {
        uv.y += sin(uv.x * _Frequency + t) * _Amplitude;
    } else {
        vec2 c = uv - 0.5;
        float d = length(c) + 1e-5;
        uv += (c / d) * sin(d * _Frequency * 2.0 - t) * _Amplitude;
    }
    fragColor = texture(iChannel0, uv);
}
