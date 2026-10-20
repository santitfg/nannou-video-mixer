// @category Palettes
// @role generator
// A horizontal color ramp (dark on the left to light on the right). Plug it
// into the second input of a Gradient Map to recolor with it, or use it as
// an image on its own.
//   Custom — three colors set by hue: start, middle, end.
Properties {
    _Palette ("Palette", Enum(Fire, Ocean, Neon, Mono, Sunset, Custom)) = 5
    _HueA    ("Start hue", Range(0, 1)) = 0.65
    _HueB    ("Middle hue", Range(0, 1)) = 0.85
    _HueC    ("End hue", Range(0, 1)) = 0.13
    _Sat     ("Saturation", Range(0, 1)) = 0.9
    _Steps   ("Steps (0 = smooth)", Range(0, 32)) = 0
    _Shift   ("Shift", Range(0, 1)) = 0.0
}

vec3 cosine(float t, vec3 a, vec3 b, vec3 c, vec3 d) {
    return a + b * cos(6.28318 * (c * t + d));
}

vec3 hsv(float h, float s, float v) {
    vec3 k = clamp(abs(fract(h + vec3(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0) - 1.0, 0.0, 1.0);
    return v * mix(vec3(1.0), k, s);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    float t = fragCoord.x / iResolution.x;
    if (_Steps > 1.0) {
        t = floor(t * _Steps) / (_Steps - 1.0);
    }
    t = fract(t + _Shift);
    vec3 ramp;
    if (_Palette == 0) {
        ramp = clamp(vec3(t * 3.0, t * 3.0 - 1.0, t * 3.0 - 2.0), 0.0, 1.0);
    } else if (_Palette == 1) {
        ramp = cosine(t, vec3(0.0, 0.3, 0.5), vec3(0.2, 0.4, 0.5), vec3(1.0), vec3(0.0, 0.1, 0.2));
    } else if (_Palette == 2) {
        ramp = cosine(t, vec3(0.5), vec3(0.5), vec3(1.0), vec3(0.0, 0.33, 0.67));
    } else if (_Palette == 3) {
        ramp = vec3(t);
    } else if (_Palette == 4) {
        ramp = cosine(t, vec3(0.6, 0.35, 0.3), vec3(0.4, 0.3, 0.3), vec3(1.0, 1.0, 0.5), vec3(0.0, 0.1, 0.25));
    } else {
        // Dark start, bright end; the middle color is the most saturated.
        vec3 a = hsv(_HueA, _Sat, 0.05 + 0.3 * t);
        vec3 b = hsv(_HueB, _Sat, 0.35 + 0.65 * t);
        vec3 c = hsv(_HueC, _Sat * 0.7, 0.6 + 0.4 * t);
        ramp = t < 0.5 ? mix(a, b, t * 2.0) : mix(b, c, t * 2.0 - 1.0);
    }
    fragColor = vec4(clamp(ramp, 0.0, 1.0), 1.0);
}
