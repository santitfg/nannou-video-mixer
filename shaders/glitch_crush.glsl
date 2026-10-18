// @category Glitch
// @role effect
// Data-corruption look: color bit-crush, channel shuffle, pixel sorting-ish
// smear along rows, and random blocks that freeze on a flat color.
Properties {
    _Amount  ("Amount", Range(0, 1)) = 0.5
    _Bits    ("Bits", Range(1, 8)) = 3.0
    _Shuffle ("Channel shuffle", Range(0, 1)) = 0.3
    _Smear   ("Smear", Range(0, 1)) = 0.3
    _Block   ("Block size", Range(8, 120)) = 48.0
    _Rate    ("Rate", Range(0, 30)) = 6.0
}

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    float t = floor(iTime * _Rate);
    vec2 block = floor(fragCoord / _Block);
    float pick = hash(block + t);
    // Smear: blocks drag their pixels to the right along the row.
    vec2 p = uv;
    if (pick < _Amount * _Smear * 0.5) {
        p.x = floor(uv.x * iResolution.x / _Block) * _Block / iResolution.x;
    }
    vec3 c = texture(iChannel0, p).rgb;
    // Channel shuffle in some blocks.
    if (hash(block * 1.3 + t) < _Amount * _Shuffle * 0.5) {
        float k = floor(hash(block + t * 2.0) * 5.99);
        if (k < 1.0) c = c.gbr;
        else if (k < 2.0) c = c.brg;
        else if (k < 3.0) c = c.bgr;
        else if (k < 4.0) c = c.grb;
        else c = c.rbg;
    }
    // Bit-crush, stronger with Amount.
    float levels = pow(2.0, mix(8.0, _Bits, _Amount));
    c = floor(c * levels + 0.5) / levels;
    // Frozen blocks.
    if (hash(block * 2.1 + t * 0.7) < _Amount * 0.03) {
        c = vec3(hash(block + 3.0), hash(block + 5.0), hash(block + 9.0));
    }
    fragColor = vec4(c, 1.0);
}
