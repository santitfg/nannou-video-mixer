// @category Color
// @role effect
// Levels (black/white points, gamma, output range) followed by hue /
// saturation / value adjustment, in one pass.
Properties {
    _InBlack  ("In black", Range(0, 1)) = 0.0
    _InWhite  ("In white", Range(0, 1)) = 1.0
    _Gamma    ("Gamma", Range(0.1, 3)) = 1.0
    _OutBlack ("Out black", Range(0, 1)) = 0.0
    _OutWhite ("Out white", Range(0, 1)) = 1.0
    _Hue      ("Hue", Range(-1, 1)) = 0.0
    _Sat      ("Saturation", Range(0, 3)) = 1.0
    _Val      ("Value", Range(0, 3)) = 1.0
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
    vec3 c = texture(iChannel0, uv).rgb;
    c = clamp((c - _InBlack) / max(_InWhite - _InBlack, 1e-4), 0.0, 1.0);
    c = pow(c, vec3(1.0 / _Gamma));
    c = mix(vec3(_OutBlack), vec3(_OutWhite), c);
    vec3 hsv = rgb2hsv(c);
    hsv.x = fract(hsv.x + _Hue);
    hsv.y = clamp(hsv.y * _Sat, 0.0, 1.0);
    hsv.z = clamp(hsv.z * _Val, 0.0, 1.0);
    fragColor = vec4(hsv2rgb(hsv), 1.0);
}
