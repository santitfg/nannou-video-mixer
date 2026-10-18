// @category Glitch
// @role effect
// Datamosh-ish hold: random blocks stop updating and keep showing the
// previous frame, so motion smears and tears. Higher Hold freezes more
// blocks for longer; Drift lets frozen blocks creep sideways.
Properties {
    _Hold  ("Hold", Range(0, 1)) = 0.4
    _Block ("Block size", Range(8, 160)) = 40.0
    _Rate  ("Rate", Range(0, 30)) = 8.0
    _Drift ("Drift", Range(0, 0.02)) = 0.002
}

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    float t = floor(iTime * _Rate);
    vec2 block = floor(fragCoord / _Block);
    vec3 now = texture(iChannel0, uv).rgb;
    float frozen = step(hash(block + t), _Hold);
    vec2 off = (vec2(hash(block + t + 1.0), hash(block + t + 2.0)) - 0.5) * _Drift;
    vec3 prev = texture(iFeedback, uv + off).rgb;
    fragColor = vec4(mix(now, prev, frozen), 1.0);
}
