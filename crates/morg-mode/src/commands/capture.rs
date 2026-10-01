use std::path::{Path, PathBuf};

use saphyr::LoadableYamlNode;

pub fn run(template_name: &str, input: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = config_file_path();

    if !config_path.exists() {
        return Err(format!(
            "Capture config not found at {}.\nCreate it with template definitions.",
            config_path.display()
        )
        .into());
    }

    let config_str = std::fs::read_to_string(&config_path)?;
    let config = saphyr::YamlOwned::load_from_str(&config_str)?
        .into_iter()
        .next()
        .ok_or("Capture config is empty")?;

    let templates = config
        .as_mapping_get("templates")
        .ok_or("No 'templates' key in capture config")?;
    let template = templates
        .as_mapping_get(template_name)
        .ok_or_else(|| format!("Template '{template_name}' not found in config"))?;

    let target_path = template
        .as_mapping_get("target")
        .and_then(|v| v.as_str())
        .ok_or("Template missing 'target' field")?;
    let target_path = expand_home(target_path);

    let template_str = template
        .as_mapping_get("template")
        .and_then(|v| v.as_str())
        .ok_or("Template missing 'template' field")?;

    let heading = template.as_mapping_get("heading").and_then(|v| v.as_str());

    let now = chrono::Local::now();
    let rendered = template_str
        .replace("{input}", input)
        .replace("{date}", &now.format("%Y-%m-%d").to_string())
        .replace("{datetime}", &now.format("%Y-%m-%dT%H:%M").to_string())
        .replace("{time}", &now.format("%H:%M").to_string());

    let rendered_heading = heading.map(|h| {
        h.replace("{input}", input)
            .replace("{date}", &now.format("%Y-%m-%d").to_string())
    });

    // Read existing file or create empty
    let mut content = if target_path.exists() {
        std::fs::read_to_string(&target_path)?
    } else {
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        String::new()
    };

    // If a heading is specified, find or create it, then append under it
    if let Some(heading_text) = rendered_heading {
        if !content.contains(&heading_text) {
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push('\n');
            content.push_str(&heading_text);
            content.push('\n');
        }
        // Append rendered content after the heading
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&rendered);
        content.push('\n');
    } else {
        // Just append to end of file
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&rendered);
        content.push('\n');
    }

    std::fs::write(&target_path, &content)?;
    println!("Captured to {}", target_path.display());

    Ok(())
}

fn config_file_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    Path::new(&home)
        .join(".config")
        .join("morg")
        .join("capture.yaml")
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Path::new(&home).join(rest)
    } else {
        PathBuf::from(path)
    }
}
