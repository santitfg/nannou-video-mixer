// @category Color
// @role effect
// Recolors the image by brightness through a color ramp (like a lookup table).
// The ramp is the built-in Palette, or any image plugged into the second
// input (its left edge = dark, its right edge = bright): a Color Ramp, a
// picture, a video...
Properties {
    _Palette ("Palette", Enum(Fire, Ocean, Neon, Mono, Sunset)) = 0
    _Shift   ("Shift", Range(0, 1)) = 0.0
    _Mix     ("Mix", Range(0, 1)) = 1.0
}

vec3 cosine(float t, vec3 a, vec3 b, vec3 c, vec3 d) {
    return a + b * cos(6.28318 * (c * t + d));
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec3 src = texture(iChannel0, uv).rgb;
    float t = fract(dot(src, vec3(0.299, 0.587, 0.114)) + _Shift);
    vec3 ramp;
    if (iChannelResolution[1].x > 0.0) {
        ramp = texture(iChannel1, vec2(t, 0.5)).rgb;
    } else if (_Palette == 0) {
        ramp = clamp(vec3(t * 3.0, t * 3.0 - 1.0, t * 3.0 - 2.0), 0.0, 1.0);
    } else if (_Palette == 1) {
        ramp = cosine(t, vec3(0.0, 0.3, 0.5), vec3(0.2, 0.4, 0.5), vec3(1.0), vec3(0.0, 0.1, 0.2));
    } else if (_Palette == 2) {
        ramp = cosine(t, vec3(0.5), vec3(0.5), vec3(1.0), vec3(0.0, 0.33, 0.67));
    } else if (_Palette == 3) {
        ramp = vec3(t);
    } else {
        ramp = cosine(t, vec3(0.6, 0.35, 0.3), vec3(0.4, 0.3, 0.3), vec3(1.0, 1.0, 0.5), vec3(0.0, 0.1, 0.25));
    }
    fragColor = vec4(mix(src, clamp(ramp, 0.0, 1.0), _Mix), 1.0);
}
