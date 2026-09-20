use std::time::Duration;
use idealyst_native::prelude::*;

#[component]
fn AfkTimerDisplay() -> impl Component {
    let minutes = 10;
    Text::new(format!("Move to AFK after {} minutes", minutes))
        .font_size(16)
        .color(Color::rgb(0.8, 0.8, 0.8))
}

// Cargo.toml should already have idealyst-native as a dependency