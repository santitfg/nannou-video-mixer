// @category Color
// @role effect
// Reduces each channel to a few flat levels.
Properties {
    _Levels ("Levels", Range(2, 16)) = 4.0
    _Gamma  ("Gamma", Range(0.3, 3)) = 1.0
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    vec3 c = pow(texture(iChannel0, uv).rgb, vec3(_Gamma));
    float n = max(_Levels, 2.0) - 1.0;
    c = floor(c * n + 0.5) / n;
    fragColor = vec4(pow(c, vec3(1.0 / _Gamma)), 1.0);
}
