use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=template");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let mut project_files: Vec<(String, String)> = Vec::new(); // written into generated project
    let mut meta_files: Vec<(String, String)> = Vec::new(); // generator metadata only

    for entry in walkdir::WalkDir::new("template")
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix("template")
            .unwrap()
            .to_str()
            .expect("non-UTF-8 template path")
            .replace('\\', "/");

        let content = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {}", path.display(), e));

        if rel == "template.yaml" || rel.starts_with(".template/") {
            // Generator metadata: available to template::load() but not written
            // into the generated project directory.
            meta_files.push((rel, content));
        } else {
            // Project generate: written verbatim (after processing) into the
            // generated project directory.
            project_files.push((rel, content));
        }
    }

    // Emit TEMPLATE_FILES  — project generate files
    let project_items = project_files
        .iter()
        .map(|(p, c)| quote::quote! { (#p, #c) });
    let project_code = quote::quote! {
        pub static TEMPLATE_FILES: &[(&str, &str)] = &[ #(#project_items),* ];
    };
    fs::write(out_dir.join("template_files.rs"), project_code.to_string())
        .expect("write template_files.rs");

    // Emit TEMPLATE_META_FILES — generator metadata files
    let meta_items = meta_files.iter().map(|(p, c)| quote::quote! { (#p, #c) });
    let meta_code = quote::quote! {
        pub static TEMPLATE_META_FILES: &[(&str, &str)] = &[ #(#meta_items),* ];
    };
    fs::write(
        out_dir.join("template_meta_files.rs"),
        meta_code.to_string(),
    )
    .expect("write template_meta_files.rs");
}
