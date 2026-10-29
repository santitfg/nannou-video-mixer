// @category Blur & Glow
// @role effect
// Chromatic blur: splits red / green / blue apart like a cheap lens while
// blurring them. Feed video into iChannel0. Plug an image into iChannel1 to
// decide where it happens: bright areas of the map get the full effect, dark
// areas stay sharp (Invert flips that). Without a map it applies everywhere.
//   Direction  Radial — splits outward from the center, stronger at the edges.
//              Angle — splits along one fixed direction.
//              Map edges — splits across the map's edges (outlines, shapes).
Properties {
    _Chroma    ("Chroma", Range(0, 0.1)) = 0.02
    _Blur      ("Blur", Range(0, 30)) = 6.0
    _Direction ("Direction", Enum(Radial, Angle, Map edges)) = 0
    _Angle     ("Angle", Range(0, 6.2832)) = 0.0
    _CX        ("Center X", Range(0, 1)) = 0.5
    _CY        ("Center Y", Range(0, 1)) = 0.5
    _MapGain   ("Map gain", Range(0, 4)) = 1.0
    _Invert    ("Invert map", Enum(No, Yes)) = 0
}

float luma(vec3 c) {
    return dot(c, vec3(0.299, 0.587, 0.114));
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    bool hasMap = iChannelResolution[1].x > 0.5;

    // How much of the effect this pixel gets, from the map's brightness.
    float m = 1.0;
    if (hasMap) {
        m = luma(textureLod(iChannel1, uv, 0.0).rgb);
        if (_Invert == 1) {
            m = 1.0 - m;
        }
        m = clamp(m * _MapGain, 0.0, 4.0);
    }

    // Direction the colors split along.
    vec2 dir;
    if (_Direction == 2 && hasMap) {
        vec2 px = 2.0 / iResolution.xy;
        float gx = luma(textureLod(iChannel1, uv + vec2(px.x, 0.0), 0.0).rgb) - luma(textureLod(iChannel1, uv - vec2(px.x, 0.0), 0.0).rgb);
        float gy = luma(textureLod(iChannel1, uv + vec2(0.0, px.y), 0.0).rgb) - luma(textureLod(iChannel1, uv - vec2(0.0, px.y), 0.0).rgb);
        vec2 g = vec2(gx, gy) * 8.0;
        float len = length(g);
        dir = len > 1.0 ? g / len : g;
    } else if (_Direction == 1) {
        dir = vec2(cos(_Angle), sin(_Angle));
    } else {
        dir = (uv - vec2(_CX, _CY)) * 1.5;
    }
    vec2 split = dir * _Chroma * m;
    vec2 blurPx = _Blur * m / iResolution.xy;

    // 32 taps: each one sits on a golden-angle disc (the blur) and at its own
    // point along the split line, weighted red on one end, green in the
    // middle and blue on the other (the rainbow).
    vec3 sum = vec3(0.0);
    vec3 total = vec3(0.0);
    for (int i = 0; i < 32; i++) {
        float fi = float(i);
        float r = sqrt((fi + 0.5) / 32.0);
        float a = fi * 2.39996;
        vec2 disc = vec2(cos(a), sin(a)) * r;
        float t = fract(fi * 0.618034) * 2.0 - 1.0; // -1..1, decorrelated from the disc
        vec3 w = vec3(
            clamp(0.5 + t, 0.0, 1.0),
            1.0 - abs(t),
            clamp(0.5 - t, 0.0, 1.0)
        ) + 0.02;
        vec3 c = texture(iChannel0, uv + split * t + disc * blurPx).rgb;
        sum += c * w;
        total += w;
    }
    fragColor = vec4(sum / total, 1.0);
}
