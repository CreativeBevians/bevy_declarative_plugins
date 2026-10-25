use bevy::{
    DefaultPlugins,
    camera::Camera3d,
    color::palettes::css::TEAL,
    scene::{SceneList, SpawnListSystem, bsn_list},
    text::TextFont,
    ui::{BackgroundColor, BorderRadius, Display, Node, UiRect, percent, px, widget::Text},
};
use bevy_app::{App, Startup};
use bevy_declarative_plugins::{DeclarativePlugin, DeclrAppExt, PluginOutput};
use bevy_ecs::hierarchy::Children;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins);
    app.add_plugins_declarative(MyApp).unwrap();
    app.run();
}

struct MyApp;

impl DeclarativePlugin for MyApp {
    fn build(&self, output: &mut PluginOutput) {
        output.add_systems(Startup, ui.spawn());
    }
}

fn ui() -> impl SceneList {
    bsn_list! {
        Camera3d
        --
        #UiLabel
        Node {
            width: percent(100),
            height: percent(100),
            display: Display::Flex,
            justify_content: bevy::ui::JustifyContent::Center,
            align_items: bevy::ui::AlignItems::Center,
        }
        Children [
            BackgroundColor(TEAL)
            Node {
                width: px(200),
                height: px(150),
                border_radius: BorderRadius::all(px(5)),
                justify_content: bevy::ui::JustifyContent::Center,
                align_items: bevy::ui::AlignItems::Center,
                display: Display::Flex,
                padding: UiRect::all(px(2)),

            }
            Children [
                Text("Declarative Plugins. Or are they? I mean there's still procedural code blocks here. Really if you were to commit to it you'd have some haskellian nonsense going on.")
                TextFont {
                    font_size: px(12),
                }
            ]
        ]
    }
}
