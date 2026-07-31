use std::env;
use std::fs;
use std::path::Path;

fn main() {
  let dir = env::args().nth(1).unwrap_or_else(|| {
    eprintln!("usage: manifest-tool <dir>");
    std::process::exit(2);
  });

  let path = Path::new(&dir);
  let mut entries: Vec<(String, u64)> = fs::read_dir(path)
    .unwrap_or_else(|err| panic!("failed to read dir '{dir}': {err}"))
    .filter_map(|entry| {
      let entry = entry.expect("failed to read dir entry");
      let name = entry.file_name().to_string_lossy().into_owned();
      if name.ends_with(".wsm") {
        let size = entry.metadata().expect("failed to stat").len();
        Some((name, size))
      } else {
        None
      }
    })
    .collect();

  entries.sort();

  let manifest = format!(
    "[{}]",
    entries
      .iter()
      .map(|(name, size)| format!(r#"{{"name":"{}","size":{}}}"#, escape(name), size))
      .collect::<Vec<_>>()
      .join(",")
  );

  fs::write(path.join("manifest.json"), manifest).unwrap_or_else(|err| panic!("failed to write manifest: {err}"));
}

fn escape(s: &str) -> String {
  s.replace('\\', "\\\\").replace('"', "\\\"")
}
