//! Reading shader files from the shaders folder: the Unity-style
//! `Properties { ... }` header plus the Shadertoy-style GLSL body.
//!
//! ```glsl
//! Properties {
//!     _Speed ("Speed", Range(0, 10)) = 1.0
//!     _Scale ("Scale", Float) = 2.0
//!     _Steps ("Steps", Int) = 4
//! }
//!
//! void mainImage(out vec4 fragColor, in vec2 fragCoord) { ... }
//! ```
//!
//! Every property becomes a global uniform of the same name in the shader
//! and a Number input port on the module.

use crate::modules::param::Param;
use std::path::{Path, PathBuf};

pub const SHADER_DIR: &str = "shaders";
pub const MAX_CHANNELS: usize = 4;
/// Where `iFeedback` (the module's previous frame) is bound.
pub const FEEDBACK_BINDING: u32 = 3 + MAX_CHANNELS as u32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PropKind {
    Float,
    Range(f32, f32),
    Int,
    /// One of `Property::options`, by index; an `int` in GLSL.
    Enum(usize),
}

#[derive(Debug, Clone)]
pub struct Property {
    pub name: String,
    pub label: String,
    pub kind: PropKind,
    pub default: f32,
    /// The choices of an `Enum` property (empty otherwise).
    pub options: Vec<String>,
}

impl Property {
    pub fn clamp(&self, v: f32) -> f32 {
        match self.kind {
            PropKind::Float => v,
            PropKind::Range(lo, hi) => v.clamp(lo.min(hi), lo.max(hi)),
            PropKind::Int => v.round(),
            PropKind::Enum(n) => v.round().clamp(0.0, n.saturating_sub(1) as f32),
        }
    }

    /// The option name for an `Enum` value.
    pub fn option_name(&self, v: f32) -> Option<&str> {
        match self.kind {
            PropKind::Enum(_) => self.options.get(self.clamp(v) as usize).map(String::as_str),
            _ => None,
        }
    }

    /// The number box for this property: a `Range` keeps its bounds;
    /// Float/Int get 0..1, widened when the default doesn't fit in it.
    pub fn param(&self) -> Param {
        let (min, max) = match self.kind {
            PropKind::Range(lo, hi) => (lo, hi),
            PropKind::Enum(n) => (0.0, n.saturating_sub(1) as f32),
            _ if (0.0..=1.0).contains(&self.default) => (0.0, 1.0),
            _ => (self.default.min(0.0), (2.0 * self.default).max(1.0)),
        };
        Param::ranged(self.default, min, max)
    }

    pub fn glsl_type(&self) -> &'static str {
        match self.kind {
            PropKind::Int | PropKind::Enum(_) => "int",
            _ => "float",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParsedShader {
    pub props: Vec<Property>,
    /// Highest `iChannelN` the code mentions, plus one (0 = none used).
    pub channels: usize,
    /// The code reads `iFeedback` (this module's own previous frame).
    pub feedback: bool,
    /// The source with the Properties block blanked out (newlines kept so
    /// line numbers stay meaningful).
    pub body: String,
}

/// Which shader node lists a file. `Any` is the legacy catch-all node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderRole {
    Any,
    Generator,
    Effect,
    Feedback,
    Mix,
}

impl ShaderRole {
    fn from_tag(tag: &str) -> Option<Self> {
        match tag.trim().to_ascii_lowercase().as_str() {
            "generator" => Some(ShaderRole::Generator),
            "effect" => Some(ShaderRole::Effect),
            "feedback" => Some(ShaderRole::Feedback),
            "mix" => Some(ShaderRole::Mix),
            _ => None,
        }
    }

    /// Whether a node of role `self` lists a file of role `file`.
    pub fn lists(self, file: ShaderRole) -> bool {
        self == ShaderRole::Any || self == file
    }
}

/// A file of the shaders folder and where it is listed.
#[derive(Debug, Clone)]
pub struct ShaderInfo {
    pub file: String,
    pub category: String,
    pub role: ShaderRole,
}

/// Reads the `// @category X` / `// @role Y` lines at the top of a file. A
/// missing role is inferred from what the code uses.
fn read_info(file: String, source: &str) -> ShaderInfo {
    let mut category = None;
    let mut role = None;
    for line in source.lines() {
        let Some(comment) = line.trim_start().strip_prefix("//") else { break };
        let comment = comment.trim();
        if let Some(c) = comment.strip_prefix("@category") {
            category = Some(c.trim().to_string()).filter(|c| !c.is_empty());
        } else if let Some(r) = comment.strip_prefix("@role") {
            role = ShaderRole::from_tag(r);
        }
    }
    let role = role.unwrap_or_else(|| match parse_shader(source) {
        Ok(p) if p.feedback => ShaderRole::Feedback,
        Ok(p) if p.channels == 0 => ShaderRole::Generator,
        Ok(p) if p.channels > 2 => ShaderRole::Mix,
        _ => ShaderRole::Effect,
    });
    ShaderInfo { file, category: category.unwrap_or_else(|| "Other".to_string()), role }
}

/// Every `.glsl`/`.frag` in the shaders folder, sorted by category then name.
pub fn scan_library() -> Vec<ShaderInfo> {
    let mut infos: Vec<ShaderInfo> = std::fs::read_dir(SHADER_DIR)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let ext = path.extension()?.to_str()?.to_ascii_lowercase();
            if ext != "glsl" && ext != "frag" {
                return None;
            }
            let name = path.file_name()?.to_str()?.to_string();
            let source = std::fs::read_to_string(&path).unwrap_or_default();
            Some(read_info(name, &source))
        })
        .collect();
    infos.sort_by(|a, b| (&a.category, &a.file).cmp(&(&b.category, &b.file)));
    infos
}

pub fn shader_path(file: &str) -> PathBuf {
    Path::new(SHADER_DIR).join(file)
}

pub fn parse_shader(source: &str) -> Result<ParsedShader, String> {
    let (props, body) = extract_properties(source)?;
    let channels = (0..MAX_CHANNELS)
        .rev()
        .find(|n| body.contains(&format!("iChannel{n}")))
        .map(|n| n + 1)
        .unwrap_or(0);
    let feedback = body.contains("iFeedback");
    Ok(ParsedShader { props, channels, feedback, body })
}

fn extract_properties(source: &str) -> Result<(Vec<Property>, String), String> {
    // The block starts at a line whose first word is `Properties`.
    let mut offset = 0;
    let mut start = None;
    for line in source.split_inclusive('\n') {
        if line.trim_start().starts_with("Properties") {
            start = Some(offset + line.find("Properties").unwrap());
            break;
        }
        offset += line.len();
    }
    let Some(start) = start else {
        return Ok((Vec::new(), source.to_string()));
    };

    let after = start + "Properties".len();
    let open = source[after..]
        .find('{')
        .map(|i| after + i)
        .filter(|&i| source[after..i].trim().is_empty())
        .ok_or("Properties: expected '{'")?;
    let close = source[open..]
        .find('}')
        .map(|i| open + i)
        .ok_or("Properties: missing closing '}'")?;

    let mut props = Vec::new();
    for (n, raw) in source[open + 1..close].lines().enumerate() {
        let line = raw.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let prop = parse_property(line).map_err(|e| format!("Properties line {}: {e}", n + 1))?;
        if props.iter().any(|p: &Property| p.name == prop.name) {
            return Err(format!("Properties: '{}' declared twice", prop.name));
        }
        props.push(prop);
    }

    let mut body = String::with_capacity(source.len());
    body.push_str(&source[..start]);
    body.extend(source[start..=close].chars().map(|c| if c == '\n' { '\n' } else { ' ' }));
    body.push_str(&source[close + 1..]);
    Ok((props, body))
}

/// `_Name ("Label", Float|Int|Range(lo, hi)|Enum(A, B, ...)) = default`
fn parse_property(line: &str) -> Result<Property, String> {
    let paren = line.find('(').ok_or("expected '(' after the name")?;
    let name = line[..paren].trim().to_string();
    let valid = !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        return Err(format!("'{name}' is not a valid identifier"));
    }

    // Find the ')' matching the first '(' (Range(...) nests one level).
    let mut depth = 0;
    let mut end = None;
    for (i, c) in line[paren..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(paren + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end.ok_or("unbalanced parentheses")?;
    let inner = &line[paren + 1..end];

    let q1 = inner.find('"').ok_or("expected a \"label\"")?;
    let q2 = inner[q1 + 1..].find('"').ok_or("unterminated \"label\"")? + q1 + 1;
    let label = inner[q1 + 1..q2].to_string();
    let type_str = inner[q2 + 1..]
        .trim_start()
        .strip_prefix(',')
        .ok_or("expected ',' then a type")?
        .trim();

    let lower = type_str.to_ascii_lowercase();
    let mut options = Vec::new();
    let kind = if lower == "float" {
        PropKind::Float
    } else if lower == "int" {
        PropKind::Int
    } else if let Some(args) = lower.strip_prefix("range") {
        let args = args.trim().strip_prefix('(').and_then(|a| a.strip_suffix(')')).ok_or("Range needs (min, max)")?;
        let (lo, hi) = args.split_once(',').ok_or("Range needs (min, max)")?;
        let lo: f32 = lo.trim().parse().map_err(|_| "bad Range min")?;
        let hi: f32 = hi.trim().parse().map_err(|_| "bad Range max")?;
        PropKind::Range(lo, hi)
    } else if lower.starts_with("enum") {
        let args = type_str[4..]
            .trim()
            .strip_prefix('(')
            .and_then(|a| a.strip_suffix(')'))
            .ok_or("Enum needs (A, B, ...)")?;
        options = args.split(',').map(|o| o.trim().to_string()).filter(|o| !o.is_empty()).collect();
        if options.is_empty() {
            return Err("Enum needs at least one option".into());
        }
        PropKind::Enum(options.len())
    } else {
        return Err(format!("unknown type '{type_str}' (use Float, Int, Range(min, max) or Enum(A, B, ...))"));
    };

    let rest = line[end + 1..].trim();
    let default = match rest.strip_prefix('=') {
        Some(v) => v
            .trim()
            .trim_end_matches([';', ','])
            .trim()
            .parse::<f32>()
            .map_err(|_| format!("bad default '{}'", v.trim()))?,
        None => 0.0,
    };

    let mut prop = Property { name, label, kind, default, options };
    prop.default = prop.clamp(prop.default);
    Ok(prop)
}

/// Bytes of the Globals uniform block, laid out to match `build_glsl`.
pub const GLOBALS_SIZE: usize = 128;

/// Wraps the user's code in the Shadertoy environment: uniforms, the
/// `iChannelN` samplers, the property uniforms and a `main()` that calls
/// `mainImage`. Returns the full source and how many lines the prelude
/// adds (so error line numbers can be mapped back to the user's file).
pub fn build_glsl(parsed: &ParsedShader) -> (String, usize) {
    let mut s = String::new();
    s.push_str("#version 450\n");
    s.push_str(
        "layout(std140, set = 0, binding = 0) uniform Globals {\n\
         vec4 iResolution4;\n\
         vec4 iMouse;\n\
         vec4 iDate;\n\
         vec4 iChannelRes4[4];\n\
         float iTime;\n\
         float iTimeDelta;\n\
         int iFrame;\n\
         float iFrameRate;\n\
         };\n",
    );
    s.push_str("layout(std140, set = 0, binding = 1) uniform Props {\n vec4 _propsPad;\n");
    for p in &parsed.props {
        s.push_str(&format!(" {} {};\n", p.glsl_type(), p.name));
    }
    s.push_str("};\n");
    s.push_str("layout(set = 0, binding = 2) uniform sampler iSamp;\n");
    for n in 0..MAX_CHANNELS {
        s.push_str(&format!(
            "layout(set = 0, binding = {}) uniform texture2D iTex{n};\n#define iChannel{n} sampler2D(iTex{n}, iSamp)\n",
            3 + n
        ));
    }
    s.push_str(&format!(
        "layout(set = 0, binding = {}) uniform texture2D iFeedbackTex;\n#define iFeedback sampler2D(iFeedbackTex, iSamp)\n",
        FEEDBACK_BINDING
    ));
    s.push_str("#define iResolution iResolution4.xyz\n#define iChannelResolution iChannelRes4\n");
    // Textures are stored top row first but Shadertoy's v runs bottom-up,
    // so flip v on every sampling call.
    s.push_str(
        "#define texture(s, uv) texture(s, vec2((uv).x, 1.0 - (uv).y))\n\
         #define textureLod(s, uv, l) textureLod(s, vec2((uv).x, 1.0 - (uv).y), l)\n\
         #define texelFetch(s, p, l) texelFetch(s, ivec2((p).x, textureSize(s, l).y - 1 - (p).y), l)\n",
    );
    s.push_str("layout(location = 0) out vec4 _fragOut;\n");
    let prelude_lines = s.matches('\n').count();

    s.push_str(&parsed.body);
    s.push_str(
        "\nvoid main() {\n\
         vec4 c = vec4(0.0, 0.0, 0.0, 1.0);\n\
         mainImage(c, vec2(gl_FragCoord.x, iResolution4.y - gl_FragCoord.y));\n\
         _fragOut = vec4(c.rgb, 1.0);\n\
         }\n",
    );
    (s, prelude_lines)
}

/// Byte size of the Props uniform block for `props`.
pub fn props_size(props: &[Property]) -> usize {
    (16 + 4 * props.len()).div_ceil(16) * 16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_properties() {
        let src = "Properties {\n  _A (\"Speed\", Range(0, 10)) = 2.5\n  _B (\"Steps\", Int) = 3 // c\n  _C (\"Plain\", Float)\n}\nvoid mainImage(out vec4 f, in vec2 p){ f = texture(iChannel1, p); }";
        let parsed = parse_shader(src).unwrap();
        assert_eq!(parsed.props.len(), 3);
        assert_eq!(parsed.props[0].kind, PropKind::Range(0.0, 10.0));
        assert_eq!(parsed.props[0].default, 2.5);
        assert_eq!(parsed.props[1].default, 3.0);
        assert_eq!(parsed.channels, 2);
        assert!(!parsed.body.contains("Properties"));
        assert_eq!(parsed.body.matches('\n').count(), src.matches('\n').count());
    }

    #[test]
    fn parses_enum_properties() {
        let src = "Properties {\n _Mode (\"Mode\", Enum(Classic, Additive, Water)) = 1\n}\nvoid mainImage(out vec4 f, in vec2 p){ f = texture(iFeedback, p); }";
        let parsed = parse_shader(src).unwrap();
        let p = &parsed.props[0];
        assert_eq!(p.kind, PropKind::Enum(3));
        assert_eq!(p.options, ["Classic", "Additive", "Water"]);
        assert_eq!(p.default, 1.0);
        assert_eq!(p.option_name(2.4), Some("Water"));
        assert_eq!((p.param().min, p.param().max), (0.0, 2.0));
        assert_eq!(p.glsl_type(), "int");
        assert!(parsed.feedback);
        assert!(parse_shader("Properties { _M (\"m\", Enum()) = 0 }").is_err());
    }

    #[test]
    fn rejects_bad_type() {
        assert!(parse_shader("Properties { _A (\"x\", Vector) = 1 }").is_err());
    }
}
