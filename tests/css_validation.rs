//! CSS validation tests
//!
//! These tests validate CSS files to catch issues like undefined CSS custom properties.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

/// Collect all CSS files from a directory
fn find_css_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip dist directory
                if path.file_name().map(|n| n == "dist").unwrap_or(false) {
                    continue;
                }
                files.extend(find_css_files(&path));
            } else if path.extension().map(|e| e == "css").unwrap_or(false) {
                files.push(path);
            }
        }
    }

    files
}

/// Extract CSS custom property definitions from content
fn extract_definitions(content: &str) -> HashSet<String> {
    let mut definitions = HashSet::new();
    let re = regex::Regex::new(r"(--[\w-]+)\s*:").unwrap();

    for cap in re.captures_iter(content) {
        definitions.insert(cap[1].to_string());
    }

    definitions
}

/// Extract CSS custom property usages from content with line numbers
fn extract_usages(content: &str) -> Vec<(String, usize)> {
    let mut usages = Vec::new();
    let re = regex::Regex::new(r"var\(\s*(--[\w-]+)").unwrap();

    for (line_num, line) in content.lines().enumerate() {
        for cap in re.captures_iter(line) {
            usages.push((cap[1].to_string(), line_num + 1));
        }
    }

    usages
}

#[test]
fn test_css_variables_are_defined() {
    let static_dir = Path::new("static");

    if !static_dir.exists() {
        // Skip if static directory doesn't exist (might be running from different directory)
        return;
    }

    let css_files = find_css_files(static_dir);

    if css_files.is_empty() {
        return;
    }

    // Collect all definitions and usages
    let mut all_definitions = HashSet::new();
    let mut all_usages: HashMap<String, Vec<(String, usize)>> = HashMap::new();

    for file in &css_files {
        let content = fs::read_to_string(file).expect("Failed to read CSS file");
        let file_name = file.to_string_lossy().to_string();

        let definitions = extract_definitions(&content);
        all_definitions.extend(definitions);

        let usages = extract_usages(&content);
        for (var_name, line) in usages {
            all_usages
                .entry(var_name)
                .or_default()
                .push((file_name.clone(), line));
        }
    }

    // Find undefined variables
    let mut undefined: Vec<(String, Vec<(String, usize)>)> = Vec::new();

    for (var_name, locations) in &all_usages {
        if !all_definitions.contains(var_name) {
            undefined.push((var_name.clone(), locations.clone()));
        }
    }

    if !undefined.is_empty() {
        let mut msg = String::from("Found undefined CSS variables:\n");

        for (var_name, locations) in &undefined {
            msg.push_str(&format!("\n  {}\n", var_name));
            for (file, line) in locations {
                msg.push_str(&format!("    → {}:{}\n", file, line));
            }
        }

        // Suggest similar defined variables
        msg.push_str("\nDefined variables that might be intended:\n");
        for (var_name, _) in &undefined {
            let var_words: Vec<&str> = var_name.trim_start_matches("--").split('-').collect();
            let similar: Vec<&String> = all_definitions
                .iter()
                .filter(|d| {
                    let def_words: Vec<&str> = d.trim_start_matches("--").split('-').collect();
                    var_words.iter().any(|w| def_words.contains(w))
                })
                .take(3)
                .collect();

            if !similar.is_empty() {
                msg.push_str(&format!(
                    "  {} → maybe: {}\n",
                    var_name,
                    similar
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }

        panic!("{}", msg);
    }
}
