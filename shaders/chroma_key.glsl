// @category Key
// @role effect
// Chroma key by hue: removes (or keeps) the pixels near Hue (0 red, .33 green,
// .66 blue). Low-saturation pixels are never keyed. Spill pulls the key color
// out of the edges. Matte shows the mask. Layer it with Blend in Add/Screen.
Properties {
    _Mode      ("Mode", Enum(Remove, Keep, Matte)) = 0
    _Hue       ("Hue", Range(0, 1)) = 0.33
    _Tolerance ("Tolerance", Range(0, 0.5)) = 0.08
    _Softness  ("Softness", Range(0, 0.3)) = 0.05
    _MinSat    ("Min saturation", Range(0, 1)) = 0.2
    _Spill     ("Spill", Range(0, 1)) = 0.5
}

vec3 rgb2hsv(vec3 c) {
    vec4 K = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    vec4 p = mix(vec4(c.bg, K.wz), vec4(c.gb, K.xy), step(c.b, c.g));
    vec4 q = mix(vec4(p.xyw, c.r), vec4(c.r, p.yzx), step(p.x, c.r));
    float d = q.x - min(q.w, q.y);
    return vec3(abs(q.z + (q.w - q.y) / (6.0 * d + 1e-10)), d / (q.x + 1e-10), q.x);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec3 c = texture(iChannel0, uv).rgb;
    vec3 hsv = rgb2hsv(c);
    float dh = abs(hsv.x - _Hue);
    dh = min(dh, 1.0 - dh);
    float key = (1.0 - smoothstep(_Tolerance, _Tolerance + _Softness + 1e-4, dh))
              * smoothstep(_MinSat, _MinSat + 0.1, hsv.y);
    float l = dot(c, vec3(0.299, 0.587, 0.114));
    // Desaturate what stays, more the closer it is to the key color.
    float near = 1.0 - smoothstep(_Tolerance, _Tolerance * 3.0 + _Softness + 0.05, dh);
    vec3 clean = mix(c, vec3(l), _Spill * near * smoothstep(_MinSat, _MinSat + 0.1, hsv.y));
    vec3 col;
    if (_Mode == 1) {
        col = c * key;
    } else if (_Mode == 2) {
        col = vec3(1.0 - key);
    } else {
        col = clean * (1.0 - key);
    }
    fragColor = vec4(col, 1.0);
}
