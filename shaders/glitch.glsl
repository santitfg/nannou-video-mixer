// @category Glitch
// @role effect
// Digital glitch: displaced blocks, torn lines, RGB split and inverted flashes.
// Extra options (all off by default): Shuffle swaps color channels in some
// blocks, Crush bit-crushes the colors, Scan darkens alternate lines,
// Jitter wobbles whole rows, Freeze repeats the same glitch pattern for
// longer.
Properties {
    _Amount ("Amount", Range(0, 1)) = 0.5
    _Block  ("Block", Range(4, 80)) = 24.0
    _Speed  ("Speed", Range(0, 30)) = 8.0
    _Split  ("RGB split", Range(0, 0.05)) = 0.01
    _Shuffle("Shuffle", Range(0, 1)) = 0.0
    _Crush  ("Crush", Range(0, 1)) = 0.0
    _Scan   ("Scan", Range(0, 1)) = 0.0
    _Jitter ("Jitter", Range(0, 1)) = 0.0
    _Freeze ("Freeze", Range(1, 8)) = 1.0
}

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    float t = floor(iTime * _Speed / _Freeze);
    vec2 block = floor(fragCoord / vec2(_Block * 4.0, _Block));
    if (hash(block + t) < _Amount * 0.3) {
        uv.x += (hash(block.yx + t * 1.3) - 0.5) * 0.2 * _Amount;
    }
    float line = hash(vec2(floor(fragCoord.y / 2.0), t));
    if (line < _Amount * 0.05) {
        uv.x += (hash(vec2(line, t)) - 0.5) * 0.1;
    }
    uv.x += (hash(vec2(floor(fragCoord.y), t)) - 0.5) * 0.02 * _Jitter;
    float split = _Split * (0.5 + _Amount) * (hash(vec2(t, 3.0)) * 2.0);
    vec3 col = vec3(
        texture(iChannel0, uv + vec2(split, 0.0)).r,
        texture(iChannel0, uv).g,
        texture(iChannel0, uv - vec2(split, 0.0)).b);
    if (hash(block * 1.7 + t) < _Amount * 0.04) {
        col = 1.0 - col;
    }
    if (hash(block * 0.9 + t + 11.0) < _Amount * _Shuffle * 0.4) {
        col = col.gbr;
    }
    if (_Crush > 0.0) {
        float levels = pow(2.0, mix(8.0, 2.0, _Crush));
        col = floor(col * levels + 0.5) / levels;
    }
    col *= 1.0 - _Scan * 0.5 * step(0.5, fract(fragCoord.y * 0.5));
    fragColor = vec4(col, 1.0);
}
