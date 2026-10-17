// Big square pixels. Size is in screen pixels.
Properties {
    _Size ("Size", Range(1, 64)) = 8.0
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 cell = vec2(max(_Size, 1.0));
    vec2 center = (floor(fragCoord / cell) + 0.5) * cell;
    fragColor = texture(iChannel0, center / iResolution.xy);
}
