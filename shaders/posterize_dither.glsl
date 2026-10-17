// Retro posterize with ordered (Bayer 4x4) dithering: few colors, smooth
// gradients. Palette picks the color set the levels are drawn from.
Properties {
    _Palette ("Palette", Enum(Full color, Gameboy, CGA, Grayscale, Sepia)) = 0
    _Levels  ("Levels", Range(2, 8)) = 3.0
    _Dither  ("Dither", Range(0, 1)) = 1.0
    _Pixel   ("Pixel size", Range(1, 8)) = 2.0
}

float bayer4(vec2 p) {
    vec2 q = mod(floor(p), 4.0);
    float x = q.x;
    float y = q.y;
    // Bayer matrix built from two 2x2 levels: (x,y) -> 0..15.
    float a = mod(x, 2.0) * 2.0 + mod(x + y, 2.0);
    float b = floor(x / 2.0) * 2.0 + mod(floor(x / 2.0) + floor(y / 2.0), 2.0);
    return (a * 4.0 + b + 0.5) / 16.0;
}

vec3 gameboy(float l) {
    if (l < 0.25) return vec3(0.06, 0.22, 0.06);
    if (l < 0.5) return vec3(0.19, 0.38, 0.19);
    if (l < 0.75) return vec3(0.55, 0.67, 0.06);
    return vec3(0.61, 0.74, 0.06);
}

vec3 cga(vec3 c) {
    // Black, cyan, magenta, white: nearest by distance.
    vec3 best = vec3(0.0);
    float dist = distance(c, best);
    if (distance(c, vec3(0.33, 1.0, 1.0)) < dist) { best = vec3(0.33, 1.0, 1.0); dist = distance(c, best); }
    if (distance(c, vec3(1.0, 0.33, 1.0)) < dist) { best = vec3(1.0, 0.33, 1.0); dist = distance(c, best); }
    if (distance(c, vec3(1.0)) < dist) { best = vec3(1.0); }
    return best;
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    float px = max(_Pixel, 1.0);
    vec2 cell = floor(fragCoord / px);
    vec2 uv = (cell + 0.5) * px / iResolution.xy;
    vec3 c = texture(iChannel0, uv).rgb;
    float n = max(_Levels, 2.0) - 1.0;
    float threshold = (bayer4(cell) - 0.5) * _Dither / n;
    vec3 q = floor((c + threshold) * n + 0.5) / n;
    float l = dot(q, vec3(0.299, 0.587, 0.114));
    vec3 col = q;
    if (_Palette == 1) {
        col = gameboy(dot(c, vec3(0.299, 0.587, 0.114)) + threshold);
    } else if (_Palette == 2) {
        col = cga(c + threshold);
    } else if (_Palette == 3) {
        col = vec3(l);
    } else if (_Palette == 4) {
        col = l * vec3(1.07, 0.88, 0.62);
    }
    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
