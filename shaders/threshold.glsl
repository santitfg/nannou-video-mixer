// @category Threshold
// @role effect
// Threshold: splits the picture at Level into a dark and a light tone.
// Binary is pure black/white-ish (tinted by the two hues), Keep color
// leaves the original colors on the light side, Stripes repeats the
// threshold in bands for a solarized look. Dither adds animated noise at
// the edge, Invert swaps sides.
Properties {
    _Mode      ("Mode", Enum(Binary, Keep color, Stripes)) = 0
    _Level     ("Level", Range(0, 1)) = 0.5
    _Softness  ("Softness", Range(0, 0.5)) = 0.0
    _Dither    ("Dither", Range(0, 1)) = 0.0
    _Invert    ("Invert", Enum(Off, On)) = 0
    _Bands     ("Bands", Range(1, 16)) = 4.0
    _LightHue  ("Light hue", Range(0, 1)) = 0.0
    _LightSat  ("Light sat", Range(0, 1)) = 0.0
    _DarkHue   ("Dark hue", Range(0, 1)) = 0.6
    _DarkSat   ("Dark sat", Range(0, 1)) = 0.0
}

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

vec3 hue(float h) {
    return clamp(abs(fract(h + vec3(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0) - 1.0, 0.0, 1.0);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec3 c = texture(iChannel0, uv).rgb;
    float l = dot(c, vec3(0.299, 0.587, 0.114));
    l += (hash(fragCoord + floor(iTime * 24.0)) - 0.5) * _Dither * 0.5;
    if (_Mode == 2) {
        l = fract(l * _Bands) ;
    }
    float m = smoothstep(_Level - _Softness, _Level + _Softness + 1e-4, l);
    if (_Invert == 1) {
        m = 1.0 - m;
    }
    vec3 light = mix(vec3(1.0), hue(_LightHue), _LightSat);
    vec3 dark = mix(vec3(0.0), hue(_DarkHue) * 0.6, _DarkSat);
    vec3 col = mix(dark, light, m);
    if (_Mode == 1) {
        col = mix(dark, c, m);
    }
    fragColor = vec4(col, 1.0);
}
