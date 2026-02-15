fn main() {
    // If building for docs.rs, DO NOT create the README files from the template
    if let Ok(env) = std::env::var("DOCS_RS") {
        if &env == "1" {
            return ();
        }
    }

    let mut readme = std::fs::read_to_string("README.template.md").unwrap();
    readme = readme.replace(
        "{{VERSION}}",
        std::env::var("CARGO_PKG_VERSION")
            .expect("version is available in build.rs")
            .as_str(),
    );

    // Generate README.md
    std::fs::write("README.md", readme).unwrap();
}
