// Sobel edges: plain white lines, rainbow neon, or dark pencil lines on white.
Properties {
    _Mode     ("Mode", Enum(Edges, Neon, Sketch)) = 0
    _Strength ("Strength", Range(0, 5)) = 1.5
    _Mix      ("Mix", Range(0, 1)) = 1.0
}

float luma(vec3 c) {
    return dot(c, vec3(0.299, 0.587, 0.114));
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec2 px = 1.0 / iResolution.xy;
    float tl = luma(texture(iChannel0, uv + px * vec2(-1.0, 1.0)).rgb);
    float t  = luma(texture(iChannel0, uv + px * vec2(0.0, 1.0)).rgb);
    float tr = luma(texture(iChannel0, uv + px * vec2(1.0, 1.0)).rgb);
    float l  = luma(texture(iChannel0, uv + px * vec2(-1.0, 0.0)).rgb);
    float r  = luma(texture(iChannel0, uv + px * vec2(1.0, 0.0)).rgb);
    float bl = luma(texture(iChannel0, uv + px * vec2(-1.0, -1.0)).rgb);
    float b  = luma(texture(iChannel0, uv + px * vec2(0.0, -1.0)).rgb);
    float br = luma(texture(iChannel0, uv + px * vec2(1.0, -1.0)).rgb);
    float gx = (tr + 2.0 * r + br) - (tl + 2.0 * l + bl);
    float gy = (tl + 2.0 * t + tr) - (bl + 2.0 * b + br);
    float e = clamp(length(vec2(gx, gy)) * _Strength, 0.0, 1.0);

    vec3 src = texture(iChannel0, uv).rgb;
    vec3 fx;
    if (_Mode == 1) {
        fx = e * (0.5 + 0.5 * cos(6.2831 * (uv.x + uv.y * 0.5 + iTime * 0.1 + vec3(0.0, 0.33, 0.67))));
    } else if (_Mode == 2) {
        fx = vec3(1.0 - e);
    } else {
        fx = vec3(e);
    }
    fragColor = vec4(mix(src, fx, _Mix), 1.0);
}
