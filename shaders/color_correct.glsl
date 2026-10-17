// Hue / saturation / brightness / contrast, with an optional invert.
Properties {
    _Hue        ("Hue", Range(-1, 1)) = 0.0
    _Saturation ("Saturation", Range(0, 3)) = 1.0
    _Brightness ("Brightness", Range(-1, 1)) = 0.0
    _Contrast   ("Contrast", Range(0, 3)) = 1.0
    _Invert     ("Invert", Range(0, 1)) = 0.0
}

vec3 rgb2hsv(vec3 c) {
    vec4 K = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    vec4 p = mix(vec4(c.bg, K.wz), vec4(c.gb, K.xy), step(c.b, c.g));
    vec4 q = mix(vec4(p.xyw, c.r), vec4(c.r, p.yzx), step(p.x, c.r));
    float d = q.x - min(q.w, q.y);
    float e = 1.0e-10;
    return vec3(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

vec3 hsv2rgb(vec3 c) {
    vec3 p = abs(fract(c.xxx + vec3(1.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0);
    return c.z * mix(vec3(1.0), clamp(p - 1.0, 0.0, 1.0), c.y);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec3 hsv = rgb2hsv(texture(iChannel0, uv).rgb);
    hsv.x = fract(hsv.x + _Hue);
    hsv.y = clamp(hsv.y * _Saturation, 0.0, 1.0);
    vec3 c = hsv2rgb(hsv);
    c = (c - 0.5) * _Contrast + 0.5 + _Brightness;
    c = mix(c, 1.0 - c, _Invert);
    fragColor = vec4(clamp(c, 0.0, 1.0), 1.0);
}
