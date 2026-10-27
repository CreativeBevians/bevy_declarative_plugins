use bevy::{
    DefaultPlugins,
    camera::Camera2d,
    scene::{SpawnListSystem, bsn_list},
    ui::{Node, percent},
};
use bevy_app::{App, Startup};
use bevy_declarative_plugins::{DeclarativePlugin, DeclrAppExt, PluginOutput};

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins);
    app.add_plugins_declarative(MainPlugin).unwrap();
    app.run();
}

struct MainPlugin;

impl DeclarativePlugin for MainPlugin {
    fn build(&self, output: &mut PluginOutput) {
        output.add_plugins(UiPlugin);
    }
}

struct UiPlugin;

impl DeclarativePlugin for UiPlugin {
    fn build(&self, output: &mut PluginOutput) {
        output.add_systems(
            Startup,
            (|| {
                bsn_list! {Camera2d -- Node {width: percent(100), height: percent(100)}}
            })
            .spawn(),
        );
    }
}
