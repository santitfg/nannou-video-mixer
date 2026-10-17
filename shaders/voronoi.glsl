// Voronoi / cellular generator (no input): flat cells, glowing edges,
// distance field, or cracked "crackle" lines. Cells drift with Speed.
Properties {
    _Mode   ("Mode", Enum(Cells, Edges, Distance, Crackle)) = 0
    _Scale  ("Scale", Range(1, 40)) = 8.0
    _Speed  ("Speed", Range(0, 3)) = 0.4
    _Width  ("Edge width", Range(0.005, 0.3)) = 0.05
    _Color  ("Color", Enum(Mono, Random, Rainbow)) = 1
}

vec2 hash2(vec2 p) {
    p = vec2(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)));
    return fract(sin(p) * 43758.5453);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    vec2 p = fragCoord / iResolution.y * _Scale;
    vec2 cell = floor(p);
    vec2 f = fract(p);
    float t = iTime * _Speed;

    // Nearest and second-nearest feature points over the 3x3 neighborhood.
    float d1 = 8.0;
    float d2 = 8.0;
    vec2 nearest = vec2(0.0);
    vec2 toNearest = vec2(0.0);
    for (int y = -1; y <= 1; y++) {
        for (int x = -1; x <= 1; x++) {
            vec2 g = vec2(float(x), float(y));
            vec2 o = hash2(cell + g);
            o = 0.5 + 0.5 * sin(t + 6.2832 * o);
            vec2 r = g + o - f;
            float d = dot(r, r);
            if (d < d1) {
                d2 = d1;
                d1 = d;
                nearest = cell + g;
                toNearest = r;
            } else if (d < d2) {
                d2 = d;
            }
        }
    }
    d1 = sqrt(d1);
    d2 = sqrt(d2);

    vec2 id = hash2(nearest);
    vec3 tint = vec3(1.0);
    if (_Color == 1) {
        tint = vec3(id, hash2(id).x);
    } else if (_Color == 2) {
        tint = 0.5 + 0.5 * cos(6.2832 * (id.x + vec3(0.0, 0.33, 0.67)));
    }

    float edge = 1.0 - smoothstep(0.0, _Width, d2 - d1);
    vec3 col;
    if (_Mode == 0) {
        col = tint * mix(1.0, 0.25, edge);
    } else if (_Mode == 1) {
        col = tint * edge;
    } else if (_Mode == 2) {
        col = tint * (1.0 - d1);
    } else {
        col = tint * smoothstep(0.0, _Width * 2.0, d2 - d1) * (0.6 + 0.4 * d1);
    }
    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
