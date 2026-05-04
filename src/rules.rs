// file: src/rules.rs
// description: Static registry of build-artifact detection rules.
//
// Each rule says "if you see a directory named `dir_name` whose parent contains
// at least one of `markers`, treat it as a build artifact of `language`".
// Marker files prevent false positives — `target/` is a meaningful match only
// next to `Cargo.toml`; bare `target/` directories elsewhere are ignored.

#[derive(Debug, Clone, Copy)]
pub enum ColorHint {
    Green,
    Orange,
    Blue,
    Yellow,
    Purple,
    Red,
}

#[derive(Debug)]
pub struct ArtifactRule {
    /// Stable identifier — also used as the database string key for this kind.
    pub name: &'static str,
    /// Human-readable label shown in the UI ("Node", "Rust", "Python", etc.).
    pub language: &'static str,
    /// Directory basename to match (exact, case-sensitive).
    pub dir_name: &'static str,
    /// At least one of these files must exist in the *parent* directory for the
    /// match to be considered a true build artifact. Empty slice = unconditional.
    pub markers: &'static [&'static str],
    /// Suggested badge color in the UI.
    pub color_hint: ColorHint,
}

impl PartialEq for ArtifactRule {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for ArtifactRule {}

/// All built-in detection rules. Order is the UI display order.
pub static RULES: &[ArtifactRule] = &[
    ArtifactRule {
        name: "node_modules",
        language: "Node",
        dir_name: "node_modules",
        markers: &["package.json"],
        color_hint: ColorHint::Green,
    },
    ArtifactRule {
        name: "rust_target",
        language: "Rust",
        dir_name: "target",
        markers: &["Cargo.toml"],
        color_hint: ColorHint::Orange,
    },
    ArtifactRule {
        name: "python_venv",
        language: "Python",
        dir_name: ".venv",
        markers: &[],
        color_hint: ColorHint::Blue,
    },
    ArtifactRule {
        name: "python_venv_alt",
        language: "Python",
        dir_name: "venv",
        markers: &["pyproject.toml", "requirements.txt", "setup.py"],
        color_hint: ColorHint::Blue,
    },
    ArtifactRule {
        name: "pycache",
        language: "Python",
        dir_name: "__pycache__",
        markers: &[],
        color_hint: ColorHint::Blue,
    },
    ArtifactRule {
        name: "next_cache",
        language: "Next.js",
        dir_name: ".next",
        markers: &["package.json"],
        color_hint: ColorHint::Purple,
    },
    ArtifactRule {
        name: "nuxt_cache",
        language: "Nuxt",
        dir_name: ".nuxt",
        markers: &["package.json"],
        color_hint: ColorHint::Purple,
    },
    ArtifactRule {
        name: "parcel_cache",
        language: "Parcel",
        dir_name: ".parcel-cache",
        markers: &["package.json"],
        color_hint: ColorHint::Purple,
    },
    ArtifactRule {
        name: "gradle_cache",
        language: "Gradle",
        dir_name: ".gradle",
        markers: &["build.gradle", "build.gradle.kts", "settings.gradle"],
        color_hint: ColorHint::Yellow,
    },
    ArtifactRule {
        name: "dotnet_bin",
        language: ".NET",
        dir_name: "bin",
        markers: &[".csproj", ".sln", ".fsproj"],
        color_hint: ColorHint::Purple,
    },
    ArtifactRule {
        name: "dotnet_obj",
        language: ".NET",
        dir_name: "obj",
        markers: &[".csproj", ".sln", ".fsproj"],
        color_hint: ColorHint::Purple,
    },
    ArtifactRule {
        name: "elixir_build",
        language: "Elixir",
        dir_name: "_build",
        markers: &["mix.exs"],
        color_hint: ColorHint::Purple,
    },
    ArtifactRule {
        name: "elixir_deps",
        language: "Elixir",
        dir_name: "deps",
        markers: &["mix.exs"],
        color_hint: ColorHint::Purple,
    },
    ArtifactRule {
        name: "composer_vendor",
        language: "PHP",
        dir_name: "vendor",
        markers: &["composer.json"],
        color_hint: ColorHint::Yellow,
    },
    ArtifactRule {
        name: "xcode_derived",
        language: "Xcode",
        dir_name: "DerivedData",
        markers: &[],
        color_hint: ColorHint::Red,
    },
    ArtifactRule {
        name: "terraform_cache",
        language: "Terraform",
        dir_name: ".terraform",
        markers: &[],
        color_hint: ColorHint::Yellow,
    },
];

/// Look up a rule by its stable name.
pub fn find(name: &str) -> Option<&'static ArtifactRule> {
    RULES.iter().find(|r| r.name == name)
}

/// Match a directory basename against the rules. Returns the first matching
/// rule whose marker file (if any) exists in `parent_dir`. None if no match
/// or marker check fails. `marker_check` is invoked at most once per match.
pub fn match_dir<F>(dir_name: &str, mut marker_check: F) -> Option<&'static ArtifactRule>
where
    F: FnMut(&'static [&'static str]) -> bool,
{
    for rule in RULES {
        if rule.dir_name != dir_name {
            continue;
        }
        if rule.markers.is_empty() || marker_check(rule.markers) {
            return Some(rule);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn rule_names_are_unique() {
        let names: Vec<_> = RULES.iter().map(|r| r.name).collect();
        let unique: HashSet<_> = names.iter().collect();
        assert_eq!(names.len(), unique.len(), "duplicate rule names found");
    }

    #[test]
    fn node_modules_rule_exists() {
        let rule = find("node_modules").expect("node_modules rule missing");
        assert_eq!(rule.dir_name, "node_modules");
        assert!(rule.markers.contains(&"package.json"));
    }

    #[test]
    fn rust_target_rule_exists() {
        let rule = find("rust_target").expect("rust_target rule missing");
        assert_eq!(rule.dir_name, "target");
        assert!(rule.markers.contains(&"Cargo.toml"));
    }

    #[test]
    fn all_rules_have_nonempty_name_and_dir() {
        for rule in RULES {
            assert!(!rule.name.is_empty(), "rule has empty name");
            assert!(!rule.dir_name.is_empty(), "rule {} has empty dir_name", rule.name);
            assert!(!rule.language.is_empty(), "rule {} has empty language", rule.name);
        }
    }
}
