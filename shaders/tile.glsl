// @category Distortion
// @role effect
// Repeats the image in a grid; Mirror flips every other tile so seams match.
Properties {
    _RepeatX ("Repeat X", Range(1, 10)) = 2.0
    _RepeatY ("Repeat Y", Range(1, 10)) = 2.0
    _Mirror  ("Mirror", Enum(Off, On)) = 0
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 p = fragCoord / iResolution.xy * vec2(_RepeatX, _RepeatY);
    vec2 cell = floor(p);
    vec2 f = fract(p);
    if (_Mirror == 1) {
        if (mod(cell.x, 2.0) > 0.5) f.x = 1.0 - f.x;
        if (mod(cell.y, 2.0) > 0.5) f.y = 1.0 - f.y;
    }
    fragColor = texture(iChannel0, f);
}
