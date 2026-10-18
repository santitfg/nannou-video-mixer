//! The bundled examples. They are compiled into the program (see
//! `build.rs`), so the user can add them to the canvas but never change
//! them: whatever is edited afterwards is a copy.

include!(concat!(env!("OUT_DIR"), "/examples_gen.rs"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExampleKind {
    /// One module (or shader category) shown working; added to the canvas.
    Box,
    /// A whole composition; replaces the canvas.
    Composition,
}

pub struct Example {
    pub kind: ExampleKind,
    pub name: String,
    pub json: &'static str,
}

/// "blur-and-glow" -> "Blur & Glow".
fn display_name(stem: &str) -> String {
    stem.replace("-and-", " & ")
        .split(|c: char| c == '-' || c == '_' || c == ' ')
        .map(|w| {
            let mut chars = w.chars();
            chars.next().map_or(String::new(), |f| f.to_uppercase().collect::<String>() + chars.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn all() -> Vec<Example> {
    EMBEDDED
        .iter()
        .map(|(group, stem, json)| Example {
            kind: if *group == "boxes" { ExampleKind::Box } else { ExampleKind::Composition },
            name: display_name(stem),
            json,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_readable() {
        assert_eq!(display_name("blur-and-glow"), "Blur & Glow");
        assert_eq!(display_name("two-cameras-blend-50-50"), "Two Cameras Blend 50 50");
    }

    #[test]
    fn there_are_both_kinds() {
        let all = all();
        assert!(all.iter().any(|e| e.kind == ExampleKind::Box));
        assert!(all.iter().any(|e| e.kind == ExampleKind::Composition));
    }
}
