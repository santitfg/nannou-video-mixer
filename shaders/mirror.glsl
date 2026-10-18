// @category Distortion
// @role effect
// Mirrors one half of iChannel0 onto the other, or one quadrant onto all four.
Properties {
    _Mode ("Mode", Enum(Left, Right, Top, Bottom, Quad)) = 0
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    if (_Mode == 0) {
        uv.x = uv.x > 0.5 ? 1.0 - uv.x : uv.x;
    } else if (_Mode == 1) {
        uv.x = uv.x < 0.5 ? 1.0 - uv.x : uv.x;
    } else if (_Mode == 2) {
        uv.y = uv.y < 0.5 ? 1.0 - uv.y : uv.y;
    } else if (_Mode == 3) {
        uv.y = uv.y > 0.5 ? 1.0 - uv.y : uv.y;
    } else {
        uv = 0.5 - abs(uv - 0.5);
    }
    fragColor = texture(iChannel0, uv);
}
