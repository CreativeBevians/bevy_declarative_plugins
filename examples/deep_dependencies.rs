//! This example shows off a deeper, circular dependency tree of the form:
//!
//! ```text
//!   A    B   C
//!  / \    \ / \
//! D   E    F   G
//! |   |   /    |
//! C   B  H     A
//!        |
//!        A
//! ```
//!
//! `A`, `B`, and `C` are the "entry points" — what the app author actually
//! registers. The behavior of these plugins is artificial, in that they don't
//! represent a likely
//!
//! Plugin Dependencies are resolved via heuristics and approval mechanisms,
//! and are explored through a breadth-first search until an approved version
//! of a plugin type is found. Plugins closer to the "roots" (or "entry
//! points") are favored vs ones further away. Unresolvable circular
//! dependencies (i.e. approval never passes, therefore search goes on forever)
//! are limited by there being a maximum number of instances of a single plugin
//! allowed.

use bevy_app::App;
use bevy_declarative_plugins::DeclrAppExt;

use crate::{a::A, b::B, c::C};

fn main() {
    let mut app = App::new();
    app.add_plugins_declarative((A, B { num: 4 }, C { num: 42 }))
        .unwrap();
    app.run();
}

mod a {
    use bevy_declarative_plugins::{DeclarativePlugin, PluginOutput};

    use crate::{d::D, e::E};

    #[derive(Debug)]
    pub struct A;

    impl DeclarativePlugin for A {
        fn build(&self, output: &mut PluginOutput) {
            dbg!(self);
            output.add_plugins((D, E));
        }
    }
}

mod b {
    use bevy_declarative_plugins::{DeclarativePlugin, PluginOutput};

    use crate::f::F;

    #[derive(Debug)]
    pub struct B {
        #[allow(dead_code)]
        pub num: usize,
    }

    impl DeclarativePlugin for B {
        fn build(&self, output: &mut PluginOutput) {
            dbg!(self);
            output.add_plugins(F);
        }
    }
}

mod c {
    use bevy_declarative_plugins::{DeclarativePlugin, PluginOutput};

    use crate::{f::F, g::G};

    #[derive(Debug)]
    pub struct C {
        #[allow(dead_code)]
        pub num: usize,
    }

    impl DeclarativePlugin for C {
        fn build(&self, output: &mut PluginOutput) {
            dbg!(self);
            output.add_plugins((G, F));
        }
    }
}

mod d {
    use bevy_declarative_plugins::{DeclarativePlugin, PluginOutput, entry_points::AddDependency};

    use crate::c::C;

    #[derive(Debug)]
    pub struct D;

    impl DeclarativePlugin for D {
        fn build(&self, output: &mut PluginOutput) {
            dbg!(self);
            output.add_plugins(C { num: 10 }.with_approval(|c| c.num % 2 == 0));
        }
    }
}

mod e {
    use bevy_declarative_plugins::{DeclarativePlugin, PluginOutput};

    use crate::b::B;

    #[derive(Debug)]
    pub struct E;

    impl DeclarativePlugin for E {
        fn build(&self, output: &mut PluginOutput) {
            dbg!(self);
            output.add_plugins(B { num: 20 });
        }
    }
}

mod f {
    use bevy_declarative_plugins::{DeclarativePlugin, PluginOutput};

    use crate::h::H;

    #[derive(Debug)]
    pub struct F;

    impl DeclarativePlugin for F {
        fn build(&self, output: &mut PluginOutput) {
            dbg!(self);
            output.add_plugins(H {
                message: "H for Hello".to_string(),
            });
        }
    }
}

mod g {
    use bevy_declarative_plugins::{DeclarativePlugin, PluginOutput};

    use crate::a::A;

    #[derive(Debug)]
    pub struct G;

    impl DeclarativePlugin for G {
        fn build(&self, output: &mut PluginOutput) {
            dbg!(self);
            output.add_plugins(A);
        }
    }
}

mod h {
    use bevy_declarative_plugins::{DeclarativePlugin, PluginOutput};

    use crate::a::A;

    #[derive(Debug)]
    pub struct H {
        #[allow(dead_code)]
        pub message: String,
    }

    impl DeclarativePlugin for H {
        fn build(&self, output: &mut PluginOutput) {
            dbg!(self);
            output.add_plugins(A);
        }
    }
}
