const COMMANDS: &[&str] = &[
  "pick_image", "pick_archive", "pick_audio", "pick_video", "save_recording", "record_screen",
];

fn main() {
  tauri_plugin::Builder::new(COMMANDS)
    .build();
}
