const COMMANDS: &[&str] = &["pick_image", "pick_archive", "pick_audio", "pick_video"];

fn main() {
  tauri_plugin::Builder::new(COMMANDS)
    .build();
}
