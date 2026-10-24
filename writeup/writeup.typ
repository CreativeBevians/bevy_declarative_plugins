#import "@preview/fletcher:0.5.8": diagram, edge, node, as-label

#set par(justify: true)
#set heading(numbering: "1.")
#set page(margin: 4em)


#let erased = math.italic("Erased")
#let peek = $"peek"$
#let bool = math.italic("bool")
#let option = math.italic("Option")
#let output = math.italic("Output")
#let dependencies = math.italic("Dependencies")
#let plugins = (
  framework: `FrameworkPlugins`,
  windowing: `Windowing`,
  graphics: `Graphics`,
  audio: `Audio`,
  user: (
    a: `UserPluginA`,
    b: `UserPluginB`,
    input: `UserInputHandler`,
    game_systems: `UserGameSystems`,
    ui: `UserCommonGameUI`,
  )
)

#let if_html(body) = context {
  if target() == "html" {
    html.frame(body)
  } else {
    body
  }
}

#let cols = 1

#columns(cols)[
= Declarative Plugins for ECS Worlds

There have been several attempts at trying to make the Bevy plugin system more expressive and feature-rich. This is yet another attempt, we will see if it actually goes anywhere.

The inspiration for this approach was first thinking about how build systems like Cargo or npm manage dependencies, as well as asking myself "how could hot-patching support the addition/removal of systems." The latter has not been addressed here, but the API surface of plugins has been made more restrictive in ways that make it possible to start reasoning about "diffing" plugin registration in future.

== Motivation

Bevy plugins can't have dependencies without worrying about what happens when things get registered more than once. This works fine if you own the entirety of what you're registering, by breaking up each module of your bevy program 

The current plugin system works as a sequence of function calls that directly modify the game world before running the game loop. This is a simple API surface to reason about until you want to develop more complex frameworks or projects where assumptions can be broken about what is registered where. This also makes developing "deep" plugins difficult: if you're developing a plugin that assumes that the Avian physics engine is present you have to register that assumption in text you assume the user will read.

This design has two main purposes:

1. Make Plugin Registration an idempotent #footnote()[i.e. $x != f(x); f(f(x)) = f(x)$, or "I painted the green wall red, then I painted it red a second time. The color only changed after the first painting."] operation
2. Exposing an API surface for "static" information for tools i.e. type/function declarations for asset BSN or an Editor.

This comes with an intent to enable the following in future:

1. Make hot-patching be able to account for adding or removing systems by 
2. Provide an surface that's "good enough" for defining new systems and types at runtime.

The current model of plugins for Bevy works like so:

1. User adds an n-tuple of plugin types to the app.
2. These are executed in order.
3. Items added by these plugins have some nuance.
]
#figure(caption: [Plugin registration.])[
```rust
let mut app = App::new();
app.add_plugins((
  FrameworkPlugins::default(), 
  UserPluginA, 
  UserPluginB,
));
app.run();
```
]<currentcode>

#figure(caption: [Plugin Registration Order Tree, presented "sideways" to highlight order of registration.])[
  #import "diagrams/current-plugin-order.typ": current-plugin-order
  #if_html(current-plugin-order())
] <currenttree>

#columns(cols)[
@currentcode expands into @currenttree, the order that these individual plugins will be executed in is determined by the order in which they are registered. The order we would see is:

1. #plugins.framework 
2. #plugins.windowing
3. #plugins.graphics
4. #plugins.audio
5. #plugins.user.a
6. #plugins.user.input
7. #plugins.user.b
8. #plugins.user.game_systems
9. #plugins.user.ui

This is because each plugin registration is a trait method of `(&self, &mut App) -> ()` whose implementations take the form:

#figure()[
```rs
impl Plugin for UserPluginB {
  fn register(&self, app: &mut App) {
    app.add_plugins(UserGameSystems);
    // Add items to world.
    app.add_systems(Update, (system_10, system_11));
    app.add_message::<MyMessage>();
    // etc.
  }
}
```
]

== Upsides of the Current Model

It's simple! Functions that add things directly to a `&mut App`, directly to `World`, is a really straightforward model for how to think about Plugins.

The ways plugins currently work doesn't take that much thought to reason about in a shallow ecosystem. It makes sense

== Downsides of the Current Model

The current model can be compared to C/C++ style "Header Files" in terms of how a Bevy developer is expected to manage constraints and risk. Each author is trusted to understand the context of plugin registration and not do anything too wild, dependencies are rare because everyone defaults to conservative behavior.

1. Plugin dependencies aren't managed.
2. Even if plugin dependencies were managed, different plugin configurations are not managed. It would be a "First-Come First Serve" model, which does not scale to deep plugin dependency trees that cross author and crate borders.
3. Bevy is coming up towards needing more broad _tooling surfaces_ for stuff like Editors.

== Potential in a Declarative Model

A declarative model puts a system between the _data_ provided by the plugin author and the _app_ that runs the Bevy app. This allows us to:

1. Resolve duplicates.
2. Enforce well-formed ordering of different kinds of data.
3. Decide to interpret that data in more ways than just "Run the Game."

(3) is of particular interest as a means to better enable game-specific tooling i.e. a "BSN language server" mode for a game program, in a world where the Plugin API is extended.

== Declarative Model / Graph Implementation

A declarative model of plugins would, rather than hand the user arbitrary code execution on the World during registration, let the user expose a data structure for Bevy to turn into a running Bevy app.

#figure(caption: [A plugin and its output.])[
  $A$ is the initial plugin data.\
  $output(A)$ is the data structure that is produced by running the plugin registration function on that data.
]

= Implementation Details

== Taxonomy of Bevy Plugins Items

- Systems.
- Observers.
- Resources.
- Schedule Labels.
- Event types (Messages and Entity Events.)
- States (if `bevy_state` is present.)
- Other plugins ("Dependencies.")

Users are expected to order their plugins and plugin items correctly, and not perform duplicate plugin registration operations.

Very few of these items have a proper ordering that needs to be considered. We can always re-order an unsound non-dependent set of items into the following order:

1. Other Plugins (We want everything we depend on to be present first.)
2. Messages / Events (Zero Dependencies)
3. States (Zero Dependencies, substates though?)
4. Schedule Labels (Zero Dependencies but might want to depend on States)
5. Observers (Hypothetically wants to know Events)
6. Systems (Wants to know Schedule Labels)
7. Resources (Wants to know Observers due to Resources As Entities)

(1) leaves us needing to recursively perform (2-7) for all prior plugins as part of the resolution tree.

(2-3) can be done all at once, without any ordering applied.

(4) can be done across all plugins, but there are dependencies between them.

(5, 7) can be done across all plugins after all event types are registered (3).

(6) can be done across all plugins after all schedule labels are registered (4)

This leaves "Plugin registration ordering" to be less of a concern. Specifically, this means we can remove (1) and have 6 rather than 7 steps with no need to worry about recursion. Instead, we can order all the subsequent parts of the dependency tree individually.

However, rather than worry about recursion we still need to worry about conflicting requirements between plugins.

== Zero-Sized Plugin Types

Most plugin types are _Zero-Sized Types_, in that they are a struct of the following form:

#figure(caption: [])[
```rust
pub struct MyFeaturePlugin;
```
]

This type has *No Fields* and *No Lifetimes*, and is indicative of most plugins.

Another kind of this plugin that is still a _Zero-Sized Type_ is one with generics and a `PhantomData` field:

#figure(caption: [])[
```rust
pub struct GenericFeaturePlugin<T>(std::marker::PhantomData<T>);
```
]

This kind of struct is still _Zero-Sized_ as the only fields it has are also all _Zero-Sized_. This lines up easily with intuition: The size of a type is at least the size of all its fields, the only reason they could be _larger_ than the sum of all fields is _alignment padding_.

We can make a few assumptions about a plugin type that is _Zero-Sized_.

1. Any two instances of a `'static` ZST are the same semantically. $A = A$
2. The output of any two instances of a ZST are the same: \
#figure(caption: [])[
$"let" a_1 = "Output"(A)\ "let" a_2 = "Output"(A)\ a_1 = a_2$
]

(2) can _technically_ be violated by users running code dependent on information available to the machine it is running on (i.e. random information, clock time). This is mediated by making these assumptions and depending on having access to the Type ID of each type, we can choose to only ever run the registration function once for _ZST_ plugins.

=== Why not refuse Non-ZST plugins?

This would greatly simplify plugin resolution down to ordering, as there would no longer be any need to resolve conflicting plugin configurations.

Current-day plugin registration has the type signature for a given plugin type:

#figure(caption: [])[
```rust
fn build(&self, &mut App) -> ()
```
]

This means that plugin fields are the "input" surface for the plugin registration function. We can imagine ZST plugins as functions of $"Unit" -> "PluginOutput"$ (where $"PluginOutput"$ contains the $"TypeId"$ of the plugin)

_Why Not Remove This?_ is answered by _People are already using them_. [citation needed]. It's also useful.

== Storing Non-`dyn` Items as Erased Data

A number of the things that need to be registered to World are not dyn-compatible. This makes _safe_ storage difficult. The MVP implementation involves using type-erased `!Sync` +`!Send` pointer paired with a "un-erase and add to world" function pointer.

#figure(caption: [])[
$
  frac(T in "Types"\, T: \'italic("static"), "typeid" : \&T -> "TypeId"\ "erase" : T -> erased \ "unerase" : erased -> "Result"(T, erased)\ "peek": \&erased -> (forall space 'a. space \&'a space T -> A) -> option(A))
$
]

$erased$ is a pointer type that stores a pointer to data on the heap, as well as a Type ID, Layout, Pointer to a Drop function, and a boolean that tracks if the pointer has already been dropped. This is a lot of information, but it lets us reliably and safely handle the erased information. The representation in Rust is as follows:

#figure(caption: [])[
```rust
struct MetadataPtr {
    layout: Layout,
    ptr: NonNull<()>,
    drop_fn: fn(NonNull<()>, Layout),
    type_id: TypeId,
    already_dropped: bool,
}
```
]

"$"erase"$" is a function that can project any $\'italic("static")$ type $T$ into the $erased$ type.

"$"unerase"$" is a function that tries to consume the $erased$ type and return it to a given owned type $T$. If this fails, either due to $"TypeId"$s not matching or the data already being dropped, the original $erased$ value is returned instead.

"#{peek}" is a function for looking at the erased value in an un-erased state. This function uses higher-ranked bounds to prevent  the closure from returning a reference to the data.

=== Un-Erasing

We can, due to un-erase, store a function pointer on data structures built up from use of $erased$ that takes both $erased$ and World and adds the $erased$ to World using well-typed methods. This lets us rely on the well-typed and well-formed parts of the Bevy API that are available today, even if tomorrow things may look different.

=== Alternative: Using `World` for Plugin Resolution.

The ECS uses similar data erasure methods, we could hypothetically use World to do all these things.

== Conflicting Requirements

Consider the following dependency tree, where $A$ and $B$ are Entry Points added by the developer of an app:

#figure(caption: [A plugin dependency graph with a conflict between two non-_ZST_ plugins.])[
  #import "diagrams/dependency-graphs.typ": conflict-req
  #if_html(conflict-req())
]<plugin-conflict>

In this case, both $D_1$ and $D_2$ are instances of the same _Plugin Type_ but they have different configurations. That is, unlike a _ZST_ Plugin, $D_1 != D_2$. This also means we cannot assume that the rest of the dependency tree that would be generated by each are themselves equivalent: $..._1 != ..._2$.

== Grouping Plugins By Type

The problem that we want to solve can quickly spiral into a subgraph search. Without any kind of heuristic to narrow the scope this would leave us dealing with an extremely complex and computationally intensive task. Consider @naive-graph:

#figure(caption:[Naive Formulation as a normal Digraph.])[
  #import "diagrams/dependency-graphs.typ": plugins-ungrouped
  #if_html(plugins-ungrouped())
]<naive-graph>

Instead of tracking which specific instance of a plugin depends on which specific instance of another plugin, we instead track which specific instance of a plugin depends on a plugin type. That is, instead of tracking $A_1 ->_"dep" C_2$, we track $A_1 ->_"dep" C$ + A function that determines if a dependency's configuration is appropriate. See: @approvalh.

#figure(caption: [Collection Graph which houses all instances of all plugins and the the dependencies between things.])[
  #if_html(diagram(
    node-stroke: 0.1em,
    node-shape: rect,
    spacing: 4em,
    node((0, 0), [
      $A$\
      ${A_1,A_2,A_3}$
      ], name: <A>),
    node((-1, 1), [$B$\ ${B_1}$], name: <B>),
    node((1, 1), [
      $C$\
      ${C_1, C_2}$
      ], name: <C>),
    edge((0, 0.2), <C>, "-|>"),
    edge((0.2, 0.2), <C>, label: [$A_1,A_2,A_3 ->_"dep" C$], "-|>"),
    edge((-0.2, 0.2), <C>, "-|>"),
    edge((-0.2, 0.2), <B>, label: [$A_1 ->_"dep" B$], "-|>"),
  
    edge((1,1), <B>, label: [$C_1,C_2 ->_"dep" B$], "-|>"),
    edge((1,1.2), <B>, "-|>")
  ))
]

This lets us track a version of plugin dependency where we do not need to compute arbitrary subgraphs, instead we only need to find one candidate per plugin type.

This is represented a hash map of the type id of a plugin and a vector of the instances of that plugin that have been registered:

```rust
HashMap<>
```

== Approval Functions <approvalh>

How do we resolve these trees then? The key lies in the "#peek" function described previously. The function we pass to "#peek" can be of type $forall space 'a. space \&'a space T -> bool$.

We define the "approval function" as a struct like so:

#figure(caption: [])[
```rust
struct Approval<T: ?Sized> {
    approval_fn: Option<Box<dyn Fn(&T) -> bool>>,
}
```
]

The assumption here is that most plugins will approve of any non-ZST plugin's value. For this reason, if `approval_fn` is `None`, it will always return true for any value.

#figure(caption: [Definitions of an approval type and its behavior in practice.])[
  $"approve" := option(T -> bool)$ <approvetype> \ 
  $"approve"(x: T) := cases(
  "true" "if" "approve" = "None",
  f(x) "if" "approve" = "Some"(f: T -> bool)
)$ <approvedef>
] <approval-maths>

#figure(caption:[Expansion of the conflict in @plugin-conflict, showing the results of the approval functions.])[
  #if_html(diagram(
    node-stroke: .1em,
    spacing: 2em,
    node((-1, 1), [$C, f : option(D -> bool)$], name: <C>),
    node((1, 1), [$B, g : option(D -> bool)$], name: <B>),
    node((-1, 6), $D_1$, name: <D1>),
    node((1, 6), $D_2$, name: <D2>),
    edge((-1, 0), <C>, "--|>"),
    edge(<C>, <D1>, "-|>"),
    edge(<B>, <D2>, "-|>"),
    edge(<B>, <D1>, "-|>-", stroke: green, label: [$g(D_1) = "true"$], bend: -15deg),
    edge(<C>, <D1>, "-|>-", stroke: green, label: [$f(D_1) = "true"$], bend: -45deg),
    edge(<C>, <D2>, "-|>-", stroke: red, label: [$f(D_2) = "false"$], bend: 15deg),
    edge(<B>, <D2>, "-|>-", stroke: green, label: [$g(D_2) = "true"$], bend: 45deg),
  ))
] <conflict-expansion>

In the above diagram, an expansion of the conflict in @plugin-conflict, we see that the approval functions $f in output(C), g in output(B)$ are applied to each instance of plugins of type $D.$

#figure(caption: [Table formulation of @conflict-expansion.])[
  #table(
    columns: (auto, auto, auto),
    align: center + horizon,
    table.header([], [$D_1$], [$D_2$]),
    $f in C$, "true", table.cell(fill: red.lighten(67%))[false],
    $g in B$, "true", "true",
    [Valid Candidate], "yes", "no"
  )
]<table-expansion>

=== Approvals Failing

For a plugin configuration to be accepted, _all possible dependents_ need to approve of it. Again, the assumption is that the vast majority of all approval functions are of the form $f(x) = "true"$. The case of a failure should be considered as a red line being crossed rather than a flimsy preference not being met.

== Graph Construction Algorithm

Rather than evaluating subgraphs, we can shortcut a good amount of checking plugin configs by not adding a plugin if any of the previously registered dependents do not approve of it as a plugin config.

If we come across a plugin config that is disapproved by anything that depends on it, then we can decide to avoid evaluating it at all.

#figure(caption: [Graph Construction Algorithm with Proactive Culling])[
  // #set edge(crossing: true)
  #import "diagrams/algo-expansion.typ": algo-expansion
  #if_html(algo-expansion())
]<graph-construction>

=== State Machine Behaviour and Plugin Instance Limits

We can construct a plugin subgraph that, left unchecked, would consume all resources of a machine it runs on.

1. `A(usize)`
2. `B(usize)`
3. A requires B(A.0 + 1), approves only if B.0 > A.0.
4. B requires A(B.0 + 1), approves only if A.0 > B.0.

We can avoid this by simply having a limit for the number of instances of a single plugin, and as the graph is constructed from entry points (distance=0) outwards this avoids a situation where game developer intent is overruled.

Detecting repeating sub-cycles in a graph may be more desirable from an academic interest standpoint, but to keep startup times in a end-user friendly timescale having a low ceiling on how many instances of a plugin can be registered before it's ignored makes more sense.

=== Many Good Candidates

Once we have our candidates for plugin configurations, we need to evaluate which one to pick.

1. Distance from an entry point.

Distance from entry point corresponds loosely to the intent of a plugin / game author. This is to say, a game developer can override the configuration of plugins that are further down their supply chain by declaring plugin dependencies closer to the entry point.

#figure(caption: [Entry point distances.])[
  #if_html(diagram(
    node-stroke: .1em,
    spacing: 1em,
    node((-1, 0), $A$, name: <A>),
    node((0, 0), $B$, name: <B>),
    node((1, 0), $C$, name: <C>),
    node((2, 0), [$D_4$\ distance = 0], name: <D4>),
    node((-1, 1), [$D_1$\ distance = 1], name: <D1>),
    node((0, 3), [$D_2$\ distance = 3], name: <D2>),
    node((1, 2), [$D_3$\ distance = 2], name: <D3>),
    node((0, 1), $E$, name: <bdots1>),
    node((0, 2), $G$, name: <bdots2>),
    node((1, 1), $F$, name: <cdots1>),
    edge(<A>, <D1>, "-|>"),
    edge(<B>, <bdots1>, "-|>"),
    edge(<bdots1>, <bdots2>, "-|>"),
    edge(<bdots2>, <D2>, "-|>"),
    edge(<C>, <cdots1>, "-|>"),
    edge(<cdots1>, <D3>, "-|>"),
  ))
]

We prefer lower distances as this is a fairly reliable proxy for _user intent_. While it's still possible for an entry point plugin to end up not approved compared to a deeper one, this should be an uncommon occurrence. TBD how this works out in practice.

== Approval Functions on Resources

There can only be one instance of a resource at any one time. This presents a similar problem to "we only want to register a plugin once" and so we can treat resources as more simple versions of plugin dependencies.

Resources, unlike plugins, cannot introduce further dependencies. This makes selection a 

== Plugins that touch the Rendering World

This model doesn't currently account for render world.

Rendering now uses systems and system ordering, so hypothetically we can just have a systems registration thing going on that just wraps passes in a "Render" schedule.

A more enticing alternative could be an alternative version of `PluginOutput` geared for rendering world. We could achieve two ways:

1. Have an additional type parameter that denotes which world it's for.
2. Have an api on the normal `PluginOutput` that can return a "render world" `PluginOutput`.

For the `DeclarativePlugin` trait we could then have an additional, optional method:

#figure(caption: [])[
```rust
trait DeclarativePlugin {
  // PluginOutput has the default type param `PluginDependencies`
  fn build(&self, output: &mut PluginOutput);

  fn render_world(&self, output: &mut PluginOutput<()>) {
    // Empty default impl.
  }
}
```
]<rendering>

This `DeclarativePlugin::render_world` provides an API surface for doing everything we can already do in the `build` method _except_ register new dependencies. This means we can have a single authoritative `PluginOutput` for the "main" world while having a sub `PluginOutput` with operations relevant only for reasoning about the Render world.

Alternatively, we could have an operation on `PluginOutput` that retrieves a `PluginOutput<()>` for the render world, or some other type 

== Future: A Declarative API

None of what's been stated here is a "true" declarative API, rather it is a builder pattern with a lot of type erasure. What this surface can lay ground for is a more declarative-focused API surface:

#figure(caption: [])[
```rust

trait 

trait DeclarativePlugin {
  fn systems(&self) -> Box<dyn Systems> {
    Box::new((
      (Update, (system_1, system_2, system_3).chain()),
      (Startup, set_camera),
      // ...
    ))
  }
}
```
]

== Editor API

Like with applying things to render world, game engine editors present a situation where a plugin might want to expose information and behaviour in a different context than usual.

== Scene Format Extensions

With this declarative model we have a point between plugin declaration and game execution where we can examine the data we have.
]