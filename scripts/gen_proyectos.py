"""Generates the example projects in proyectos/.

    python scripts/gen_proyectos.py proyectos

Each project: 3 camera lanes with light effects -> mixer.glsl -> Output,
plus a WLED -> Map -> Select group left unwired for manual patching.
"""
import json, os, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "proyectos"
CAM = "USB2.0 HD UVC WebCam"
# Workaround: several Camera modules on one device fight over it (see
# TODO.md, "la misma cámara en varios módulos"). While True, every lane is fed
# by Cam 1; Cam 2 and Cam 3 are still created, just left unwired.
SHARED_CAM = True

# shader file -> (kind, category, {prop: (min, max)})
SH = {
    "chroma_shift": ("effect", "Glitch", {"_Amount": (0, 0.1), "_Angle": (0, 6.2832)}),
    "feedback": ("feedback", "Feedback", {"_Mode": (0, 5), "_Feedback": (0, 0.99), "_Zoom": (0.9, 1.1), "_Rotate": (-0.1, 0.1), "_Distort": (0, 1)}),
    "threshold": ("effect", "Threshold", {"_Mode": (0, 2), "_Level": (0, 1), "_Softness": (0, 0.5), "_Dither": (0, 1), "_Invert": (0, 1), "_Bands": (1, 16),
                                         "_LightHue": (0, 1), "_LightSat": (0, 1), "_DarkHue": (0, 1), "_DarkSat": (0, 1)}),
    "edge_detect": ("effect", "Stylize", {"_Mode": (0, 2), "_Strength": (0, 5), "_Mix": (0, 1)}),
    "color_correct": ("effect", "Color", {"_Hue": (-1, 1), "_Saturation": (0, 3), "_Brightness": (-1, 1), "_Contrast": (0, 3), "_Invert": (0, 1)}),
    "levels_hsv": ("effect", "Color", {"_InBlack": (0, 1), "_InWhite": (0, 1), "_Gamma": (0.1, 3), "_OutBlack": (0, 1), "_OutWhite": (0, 1),
                                       "_Hue": (-1, 1), "_Sat": (0, 3), "_Val": (0, 3)}),
    "posterize": ("effect", "Color", {"_Levels": (2, 16), "_Gamma": (0.3, 3)}),
    "mirror": ("effect", "Distortion", {"_Mode": (0, 4)}),
    "gradient_map": ("effect", "Color", {"_Palette": (0, 4), "_Shift": (0, 1), "_Mix": (0, 1)}),
    "posterize_dither": ("effect", "Color", {"_Palette": (0, 4), "_Levels": (2, 8), "_Dither": (0, 1), "_Pixel": (1, 8)}),
    "glitch_slice": ("effect", "Glitch", {"_Amount": (0, 1), "_Slices": (2, 60), "_Rate": (0, 30), "_Roll": (0, 1), "_Tracking": (0, 1), "_Split": (0, 0.05)}),
    "crt": ("effect", "Stylize", {"_Scanlines": (0, 1), "_Curvature": (0, 0.5), "_Vignette": (0, 1), "_Noise": (0, 0.5)}),
    "luma_key": ("effect", "Key", {"_Mode": (0, 2), "_Threshold": (0, 1), "_Softness": (0, 0.5)}),
    "kaleidoscope": ("effect", "Distortion", {}),
    "difference_mix": ("shadermix", "Mixing", {"_Mode": (0, 3), "_Gain": (0, 12), "_Threshold": (0, 1), "_Softness": (0, 0.5), "_Hue": (0, 1), "_Amount": (0, 1)}),
    "luma_range": ("effect", "Key", {"_Mode": (0, 2), "_Low": (0, 1), "_High": (0, 1), "_Softness": (0, 0.5), "_Recolor": (0, 1), "_Hue": (0, 1)}),
    "chroma_key": ("effect", "Key", {"_Mode": (0, 2), "_Hue": (0, 1), "_Tolerance": (0, 0.5), "_Softness": (0, 0.3), "_MinSat": (0, 1), "_Spill": (0, 1)}),
    "color_ramp": ("generator", "Palettes", {"_Palette": (0, 5), "_HueA": (0, 1), "_HueB": (0, 1), "_HueC": (0, 1), "_Sat": (0, 1), "_Steps": (0, 32), "_Shift": (0, 1)}),
    "mixer": ("shadermix", "Mixing", {"_Mode": (0, 4), "_Level1": (0, 1), "_Level2": (0, 1), "_Level3": (0, 1), "_Level4": (0, 1), "_Select": (0, 3)}),
}
# Video inputs per shader (iChannel ports come before the property ports).
CH = {"gradient_map": 2, "difference_mix": 2, "mixer": 4, "color_ramp": 0}
MIXER_MODES = {"average": 0, "add": 1, "screen": 2, "lighten": 3, "switch": 4}


class P:
    def __init__(self, name):
        self.name, self.mods, self.cons, self.groups, self.next = name, [], [], [], 1
        self.files = {}
        self.mods.append(self.base(0, "output", 520, 0))

    def base(self, id, kind, x, y, preview=True):
        return {"id": id, "kind": kind, "pos": [float(x), float(y)], "minimized": False, "show_preview": preview}

    def add(self, m):
        m["id"] = self.next
        self.next += 1
        self.mods.append(m)
        return m["id"]

    def camera(self, x, y):
        m = self.base(0, "camera", x, y)
        m["camera_name"] = CAM
        return self.add(m)

    def shader(self, file, x, y, **params):
        """A param is a value, or (value, min, max) to narrow its box: a
        wired LFO's 0..1 is mapped onto the box's range."""
        kind, cat, props = SH[file]
        m = self.base(0, kind, x, y)
        m["shader_file"] = file + ".glsl"
        m["shader_category"] = cat
        if params:
            def box(k, v):
                v, lo, hi = v if isinstance(v, tuple) else (v, *props[k])
                return {"value": float(v), "min": float(lo), "max": float(hi), "clamp": True}
            m["shader_params"] = {k: box(k, v) for k, v in params.items()}
        id = self.add(m)
        self.files[id] = file
        return id

    def port(self, id, prop):
        """Input index of a shader's property: video channels come first."""
        file = self.files[id]
        return CH.get(file, 1) + list(SH[file][2]).index(prop)

    def lfo(self, target, prop, freq, wave, x, y):
        m = self.base(0, "lfo", x, y, False)
        m["freq"] = freq
        m["wave"] = wave
        id = self.add(m)
        self.wire(id, target, self.port(target, prop))
        return id

    def blend(self, x, y, mode, mix):
        m = self.base(0, "blend", x, y)
        m["mix"] = mix
        m["param"] = {"value": mix, "min": 0.0, "max": 1.0, "clamp": True}
        m["blend_mode"] = mode
        return self.add(m)

    def wire(self, a, b, port=0):
        self.cons.append({"from": a, "to": b, "to_input": port})

    def group(self, title, members):
        self.groups.append({"title": title, "members": members, "collapsed": False})

    def wled(self):
        """WLED -> Map -> Select, outputs left free to patch by hand."""
        w = self.add(self.base(0, "wled", -820, -720, False))
        mp = self.add(self.base(0, "map", -560, -720, False))
        s = self.add(self.base(0, "select", -300, -720, False))
        self.wire(w, mp)
        self.wire(mp, s)
        self.group("WLED (control)", [w, mp, s])

    def save(self, fname):
        data = {"name": self.name, "output_id": 0, "modules": self.mods, "connections": self.cons, "groups": self.groups}
        with open(os.path.join(OUT, fname), "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2, ensure_ascii=False)


COL = [-900, -620, -340]  # camera, effect 1, effect 2 / blend
ROW = [330, 0, -330]       # cam 1, 2, 3


def lane(p, i, chain, first_cam):
    """chain: list of steps. ('fx', file, params) applies in series;
    ('sum', mode, mix) blends the result back over the untouched camera;
    ('mix2', file, params) runs a two-input mix shader (A = camera, B = result);
    ('lfo', prop, freq, wave) animates a property of the previous shader;
    ('ramp', params) feeds a Color Ramp into the previous shader's 2nd input."""
    y = ROW[i]
    cam = p.camera(COL[0], y)
    src = first_cam if (SHARED_CAM and first_cam is not None) else cam
    ids, last, x = [cam], src, COL[1]
    for step in chain:
        if step[0] == "lfo":
            ids.append(p.lfo(last, step[1], step[2], step[3], x - 280, y + 120))
            continue
        if step[0] == "ramp":
            r = p.shader("color_ramp", x - 280, y + 120, **step[1])
            p.wire(r, last, 1)
            ids.append(r)
            continue
        if step[0] == "fx":
            n = p.shader(step[1], x, y, **step[2])
            p.wire(last, n)
        elif step[0] == "mix2":
            n = p.shader(step[1], x, y, **step[2])
            p.wire(src, n, 0)
            p.wire(last, n, 1)
        else:  # sum with original: A = camera, B = processed
            n = p.blend(x, y, step[1], step[2])
            p.wire(src, n, 0)
            p.wire(last, n, 1)
        ids.append(n)
        last, x = n, x + 280
    p.group(f"Cam {i + 1}", ids)
    return cam, last


def project(name, fname, lanes, mode, levels, mixer_extra=None, mixer_lfo=None):
    p = P(name)
    outs, first_cam = [], None
    for i, c in enumerate(lanes):
        cam, out = lane(p, i, c, first_cam)
        first_cam = first_cam or cam
        outs.append(out)
    mx = p.shader("mixer", 220, 0, _Mode=MIXER_MODES[mode], _Level1=levels[0], _Level2=levels[1], _Level3=levels[2],
                  **(mixer_extra or {}))
    for i, o in enumerate(outs):
        p.wire(o, mx, i)
    p.wire(mx, 0)
    members = [mx]
    if mixer_lfo:  # (prop, freq, wave)
        members.append(p.lfo(mx, *mixer_lfo, 220, 300))
    p.group("Mixer", members)
    p.wled()
    p.save(fname)


fx = lambda f, **kw: ("fx", f, kw)
sum_ = lambda mode, mix: ("sum", mode, mix)
mix2 = lambda f, **kw: ("mix2", f, kw)
lfo = lambda prop, freq, wave="sine": ("lfo", prop, freq, wave)
ramp = lambda **kw: ("ramp", kw)

project("Prisma", "01-prisma.json", [
    [fx("chroma_shift", _Amount=0.008, _Angle=0.0)],
    [fx("edge_detect", _Mode=1, _Strength=1.5, _Mix=1.0), sum_("screen", 0.6)],
    [fx("color_correct", _Saturation=1.25, _Contrast=1.1)],
], "screen", [0.8, 0.8, 0.8])

project("Eco", "02-eco.json", [
    [fx("feedback", _Mode=0, _Feedback=0.75, _Zoom=1.004, _Rotate=0.002, _Distort=0.0)],
    [fx("threshold", _Mode=0, _Level=0.5, _LightHue=0.55, _LightSat=0.6), sum_("add", 0.35)],
    [fx("levels_hsv", _Gamma=0.9, _Sat=1.2)],
], "average", [1.0, 1.0, 1.0])

project("Tinta", "03-tinta.json", [
    [fx("edge_detect", _Mode=2, _Strength=1.5, _Mix=1.0), sum_("multiply", 0.7)],
    [fx("posterize", _Levels=6, _Gamma=1.0)],
    [fx("chroma_shift", _Amount=0.012, _Angle=1.5708)],
], "lighten", [1.0, 1.0, 1.0])

project("Espejo", "04-espejo.json", [
    [fx("mirror", _Mode=0), fx("chroma_shift", _Amount=0.005, _Angle=0.8)],
    [fx("feedback", _Mode=3, _Feedback=0.6, _Zoom=1.0, _Rotate=0.0, _Distort=0.2)],
    [fx("threshold", _Mode=2, _Level=0.5, _Bands=4), sum_("overlay", 0.4)],
], "screen", [0.7, 0.7, 0.7])

project("Pulso", "05-pulso.json", [
    [fx("feedback", _Mode=1, _Feedback=0.5, _Zoom=1.01, _Rotate=0.0, _Distort=0.0), fx("color_correct", _Contrast=1.15)],
    [fx("edge_detect", _Mode=0, _Strength=2.0, _Mix=1.0), sum_("add", 0.5)],
    [fx("threshold", _Mode=1, _Level=0.4, _Softness=0.05), sum_("mix", 0.5)],
], "add", [0.45, 0.45, 0.45])

project("Rastro", "06-rastro.json", [
    [fx("feedback", _Mode=5, _Feedback=0.55, _Zoom=1.0, _Rotate=0.0, _Distort=0.2)],
    # Motion highlights: the camera vs. a lagging feedback copy of itself, over the original.
    [fx("feedback", _Mode=0, _Feedback=0.9, _Zoom=1.0, _Rotate=0.0, _Distort=0.0),
     mix2("difference_mix", _Mode=2, _Gain=4.0, _Threshold=0.08, _Hue=0.9, _Amount=0.8)],
    [fx("gradient_map", _Palette=2, _Shift=0.0, _Mix=0.35)],
], "screen", [0.75, 0.75, 0.75])

project("Señal", "07-senal.json", [
    [fx("glitch_slice", _Amount=0.15, _Slices=14, _Rate=4.0, _Roll=0.0, _Tracking=0.15, _Split=0.004)],
    [fx("crt", _Scanlines=0.3, _Curvature=0.05, _Vignette=0.3, _Noise=0.02)],
    [fx("edge_detect", _Mode=1, _Strength=1.5, _Mix=1.0), sum_("add", 0.4)],
], "screen", [0.75, 0.75, 0.75])

project("Calor", "08-calor.json", [
    [fx("gradient_map", _Palette=0, _Shift=0.0, _Mix=0.4)],
    [fx("threshold", _Mode=0, _Level=0.55, _LightHue=0.08, _LightSat=0.8, _DarkHue=0.7, _DarkSat=0.5), sum_("screen", 0.4)],
    [fx("chroma_shift", _Amount=0.006, _Angle=3.1416), fx("color_correct", _Saturation=1.2)],
], "lighten", [1.0, 1.0, 1.0])

project("Caleido", "09-caleido.json", [
    [fx("kaleidoscope")],
    [fx("mirror", _Mode=4), fx("chroma_shift", _Amount=0.004, _Angle=0.0)],
    [fx("luma_key", _Mode=0, _Threshold=0.35, _Softness=0.1), sum_("lighten", 0.6)],
], "average", [1.0, 1.0, 1.0])

project("Grano", "10-grano.json", [
    [fx("posterize_dither", _Palette=0, _Levels=6, _Dither=0.3, _Pixel=1)],
    [fx("feedback", _Mode=4, _Feedback=0.4, _Zoom=1.02, _Rotate=0.0, _Distort=0.0)],
    [fx("edge_detect", _Mode=2, _Strength=1.5, _Mix=1.0), sum_("multiply", 0.6)],
], "average", [1.0, 1.0, 1.0])

# --- Color --------------------------------------------------------------
# Slow LFOs move hue / palettes; box ranges are narrowed so they only sway.

project("Aurora", "11-aurora.json", [
    [fx("color_correct", _Hue=(0.0, -0.2, 0.2), _Saturation=1.3), lfo("_Hue", 0.05)],
    [fx("gradient_map", _Palette=1, _Shift=0.0, _Mix=0.45), lfo("_Shift", 0.03, "triangle")],
    [fx("luma_range", _Mode=0, _Low=0.55, _High=1.0, _Softness=0.15, _Recolor=1.0, _Hue=0.85)],
], "lighten", [1.0, 1.0, 1.0])

project("Duotono", "12-duotono.json", [
    [fx("gradient_map", _Mix=0.6), ramp(_Palette=5, _HueA=0.62, _HueB=0.9, _HueC=0.12, _Sat=0.85)],
    [fx("threshold", _Mode=0, _Level=0.5, _Softness=0.25, _LightHue=0.12, _LightSat=0.7, _DarkHue=0.62, _DarkSat=0.7),
     sum_("overlay", 0.5)],
    [fx("levels_hsv", _InBlack=0.05, _Gamma=0.9, _Hue=0.08, _Sat=1.4)],
], "average", [1.0, 1.0, 1.0])

project("Cromático", "13-cromatico.json", [
    [fx("chroma_shift", _Amount=0.007, _Angle=0.0), lfo("_Angle", 0.08, "rampup")],
    # Reds pop: only the red areas, added back over the camera.
    [fx("chroma_key", _Mode=1, _Hue=0.0, _Tolerance=0.12, _Softness=0.08, _MinSat=0.25), sum_("add", 0.5)],
    [fx("levels_hsv", _Hue=0.5, _Sat=1.1), sum_("mix", 0.35)],
], "screen", [0.65, 0.65, 0.65])

project("Ciclo de tono", "14-ciclo-tono.json", [
    [fx("color_correct", _Hue=(0.0, -1.0, 1.0), _Saturation=1.2), lfo("_Hue", 0.04, "triangle")],
    [fx("color_correct", _Hue=0.33, _Saturation=1.2)],
    [fx("color_correct", _Hue=0.66, _Saturation=1.2)],
], "switch", [1.0, 1.0, 1.0],
    # Switch crossfades through the three cameras, driven by a slow LFO.
    mixer_extra={"_Select": (0.0, 0.0, 2.0)}, mixer_lfo=("_Select", 0.05, "sine"))

project("Saturado", "15-saturado.json", [
    [fx("edge_detect", _Mode=1, _Strength=1.5, _Mix=1.0), sum_("screen", 0.5)],
    [fx("gradient_map", _Palette=4, _Shift=0.0, _Mix=0.5)],
    [fx("color_correct", _Saturation=1.6, _Contrast=1.15, _Brightness=-0.05)],
], "lighten", [1.0, 1.0, 1.0])
print("ok")
