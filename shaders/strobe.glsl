// @category Glitch
// @role effect
// Flashes Rate times a second: white, black, or the inverted image.
// Duty is how much of each flash cycle is "on".
Properties {
    _Mode ("Mode", Enum(White, Black, Invert)) = 0
    _Rate ("Rate", Range(0, 30)) = 8.0
    _Duty ("Duty", Range(0, 1)) = 0.3
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec3 c = texture(iChannel0, fragCoord / iResolution.xy).rgb;
    bool on = _Rate > 0.0 && fract(iTime * _Rate) < _Duty;
    if (on) {
        if (_Mode == 0) c = vec3(1.0);
        else if (_Mode == 1) c = vec3(0.0);
        else c = 1.0 - c;
    }
    fragColor = vec4(c, 1.0);
}
