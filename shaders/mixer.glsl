// @category Mixing
// @role mix
// Four-input video mixer: a level for each input, and how they combine.
// Switch crossfades between inputs by Select (0..3), like a vision mixer.
// Unplugged inputs are ignored.
Properties {
    _Mode   ("Mode", Enum(Average, Add, Screen, Lighten, Switch)) = 0
    _Level1 ("Level 1", Range(0, 1)) = 1.0
    _Level2 ("Level 2", Range(0, 1)) = 1.0
    _Level3 ("Level 3", Range(0, 1)) = 1.0
    _Level4 ("Level 4", Range(0, 1)) = 1.0
    _Select ("Select", Range(0, 3)) = 0.0
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 uv = fragCoord / iResolution.xy;
    // An input with no resolution isn't plugged in.
    vec4 on = vec4(
        step(1.0, iChannelResolution[0].x),
        step(1.0, iChannelResolution[1].x),
        step(1.0, iChannelResolution[2].x),
        step(1.0, iChannelResolution[3].x));
    vec4 level = vec4(_Level1, _Level2, _Level3, _Level4) * on;
    vec3 c0 = texture(iChannel0, uv).rgb * level.x;
    vec3 c1 = texture(iChannel1, uv).rgb * level.y;
    vec3 c2 = texture(iChannel2, uv).rgb * level.z;
    vec3 c3 = texture(iChannel3, uv).rgb * level.w;

    vec3 col;
    if (_Mode == 1) {
        col = c0 + c1 + c2 + c3;
    } else if (_Mode == 2) {
        col = 1.0 - (1.0 - c0) * (1.0 - c1) * (1.0 - c2) * (1.0 - c3);
    } else if (_Mode == 3) {
        col = max(max(c0, c1), max(c2, c3));
    } else if (_Mode == 4) {
        // Weight of each input falls off linearly with distance from Select.
        vec4 w = max(vec4(0.0), 1.0 - abs(vec4(0.0, 1.0, 2.0, 3.0) - _Select)) * on;
        col = (c0 * w.x + c1 * w.y + c2 * w.z + c3 * w.w) / max(dot(w, vec4(1.0)), 1e-4);
    } else {
        col = (c0 + c1 + c2 + c3) / max(dot(on, vec4(1.0)), 1.0);
    }
    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
