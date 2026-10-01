use std::path::PathBuf;

use saphyr::{MappingOwned, Yaml, YamlEmitter, YamlOwned};

use crate::collect;

pub fn run(paths: &[PathBuf]) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = collect::parse_files(paths);

    let mut merged = YamlOwned::Mapping(MappingOwned::new());

    let mut count = 0;
    for pf in &parsed {
        if let Some(ref fm) = pf.document.frontmatter {
            deep_merge(&mut merged, &fm.data);
            count += 1;
        }
    }

    if count == 0 {
        println!("No frontmatter found.");
        return Ok(());
    }

    let yaml = emit_yaml(&merged)?;
    print!("{yaml}");
    eprintln!("---\nMerged frontmatter from {count} file(s).");

    Ok(())
}

/// Emit a YAML document as a string, without the `---` document marker the
/// emitter writes, and with a trailing newline.
fn emit_yaml(value: &YamlOwned) -> Result<String, Box<dyn std::error::Error>> {
    let mut out = String::new();
    YamlEmitter::new(&mut out).dump(&Yaml::from(value))?;
    let body = out.strip_prefix("---\n").unwrap_or(&out);
    let mut yaml = body.to_string();
    if !yaml.ends_with('\n') {
        yaml.push('\n');
    }
    Ok(yaml)
}

fn deep_merge(base: &mut YamlOwned, overlay: &YamlOwned) {
    match (base, overlay) {
        (YamlOwned::Mapping(base_map), YamlOwned::Mapping(overlay_map)) => {
            for (key, overlay_val) in overlay_map {
                if let Some(base_val) = base_map.get_mut(key) {
                    deep_merge(base_val, overlay_val);
                } else {
                    base_map.insert(key.clone(), overlay_val.clone());
                }
            }
        }
        (YamlOwned::Sequence(base_seq), YamlOwned::Sequence(overlay_seq)) => {
            base_seq.extend(overlay_seq.iter().cloned());
        }
        (base, overlay) => {
            *base = overlay.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saphyr::LoadableYamlNode;

    fn yaml(s: &str) -> YamlOwned {
        YamlOwned::load_from_str(s).unwrap().remove(0)
    }

    #[test]
    fn test_deep_merge_maps() {
        let mut base = yaml("a: 1\nb: 2");
        let overlay = yaml("b: 3\nc: 4");
        deep_merge(&mut base, &overlay);

        assert_eq!(base.as_mapping_get("a").unwrap().as_integer(), Some(1));
        assert_eq!(base.as_mapping_get("b").unwrap().as_integer(), Some(3));
        assert_eq!(base.as_mapping_get("c").unwrap().as_integer(), Some(4));
    }

    #[test]
    fn test_deep_merge_sequences() {
        let mut base = yaml("tags:\n  - a\n  - b");
        let overlay = yaml("tags:\n  - c");
        deep_merge(&mut base, &overlay);

        let tags = base.as_mapping_get("tags").unwrap().as_sequence().unwrap();
        assert_eq!(tags.len(), 3);
    }

    #[test]
    fn test_deep_merge_nested() {
        let mut base = yaml("meta:\n  author: Alice\n  version: 1");
        let overlay = yaml("meta:\n  version: 2\n  license: MIT");
        deep_merge(&mut base, &overlay);

        let meta = base.as_mapping_get("meta").unwrap();
        assert_eq!(
            meta.as_mapping_get("author").unwrap().as_str(),
            Some("Alice")
        );
        assert_eq!(
            meta.as_mapping_get("version").unwrap().as_integer(),
            Some(2)
        );
        assert_eq!(
            meta.as_mapping_get("license").unwrap().as_str(),
            Some("MIT")
        );
    }

    #[test]
    fn test_emit_yaml_strips_document_marker() {
        let value = yaml("a: 1\nb:\n  - x");
        let out = emit_yaml(&value).unwrap();
        assert!(!out.starts_with("---"));
        assert!(out.ends_with('\n'));
        assert!(out.contains("a: 1"));
    }
}
