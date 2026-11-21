#import "@preview/fletcher:0.5.8": diagram, edge, node, as-label

#set par(justify: true)
#set heading(numbering: "1.")
// #set page(margin: 4em)
#show link: underline

#let erased = math.italic("Erased")
#let peek = $"visit"$
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

#title()[Declarative Plugins for ECS Worlds - Draft]

#columns(cols)[

= Intro

The inspiration for this approach was first thinking about how build systems like Cargo or npm manage dependencies, as well as asking myself "how could hot-patching support the addition/removal of systems." The latter has not been addressed here, but the API surface of plugins has been made more restrictive in ways that make it possible to start reasoning about "diffing" plugin registration in future.

A declarative model of plugins would, rather than hand the user arbitrary code execution on the `World` during registration, let the user expose a data structure for Bevy to turn into a running Bevy app.

= Motivation

Every plugin registration function in Bevy has access to `&mut App`. This also gives every plugin registration function access to `&mut World`, the "ECS" world that stores all the ECS-related data and runs all the systems.

Because every plugin registration function has full access, Bevy plugins can't have "dependencies" without worrying about what happens when things get registered more than once. This works fine if you own the entirety of what you're registering or if your 3rd party bevy dependencies do not register any dependencies you do. If either of these invariants are not upheld then the program crashes, and non-crashing behaviour is hard to design.

We can imagine easy non-crashing behaviour that still means a developer's preferences get overwritten because a prior instance of a plugin added something and when Bevy sees the a new instance of something that already exists, it skips it. Or we can imagine giving a similar mechanism _recency bias_, but this would still require that developers be aware of the call tree of plugin registration.

This design approaches this complexity with three main goals:

1. Separate plugin registration from modifying `&mut World` or `&mut App` directly.
2. Respect the practical needs of people developing Bevy apps, including how their app-authorship-level decisions should be prioritised over those of plugin or engine authors.
3. Match the current API shape as much as possible.

This comes with an intent to enable the following in future:

1. Make hot-patching be able to account for adding or removing systems, other Bevy items, or even entire plugins.
2. Provide static/semi-static analysis tools on Bevy projects.
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

#figure()[
```rs
impl Plugin for UserPluginB {
  fn register(&self, app: &mut App) {
    // Add another plugin to the app.
    app.add_plugins(UserGameSystems);
    // Add items to world.
    app.add_systems(Update, (system_10, system_11));
    app.add_message::<MyMessage>();
    // etc.
  }
}
```
]

= Terminology

- Item: An additive change we want to make to `World`, usually representing "add this System" or "I need this message type."
- Entry Points: The plugins that an app author would add to the ECS `World`. These are at the "Root" of the dependency graph.
- Unsolved Graph: A graph whose nodes each contain a list of possible candidate plugins of the same type.
- Solved Graph: A graph whose nodes are single, unique candidates of each plugin type.
- Staged Item: Pairing of an erased Item and a function that can reverse the type erasure and add that item to an ECS `World`.
- Plugin Instance: A value of a type that implements `DeclarativePlugin`/`Plugin`.

= Data Structures

The transform we want to pursue is going from a list of "Entry Points," to a graph, to a "solved" graph, to a list of items we can register with an ECS world.

Here we'll work backwards, from answering "how do we erase and then register items with `World`" back through solved vs unsolved graphs. These do not represent the user-facing API, but the mechanisms that make the API and expected behaviour possible.

== Working Backwards: Items we'll Register

"Items" are the fundamental "Bevy things" we add during plugin registration. If it's something that introduces a piece of app-relevant information to `World`, it's an Item. An Item is an _additive_ piece of information, it introduces information and erases no existing information.

We want to be able to hand off item types to a final `World` in an order we decide once we've performed all our processing. To do this, we'll represent the items we're registering as an enum, and we'll store those items in a `Vec`:

#figure(caption: [Enum describing what we can register with an `World` during plugin registration.])[
```rust
enum DeclrItem {
    Message(MessageRegistration),
    System(StagedSystem),
    ScheduleLabel(StagedScheduleLabel),
    Observer(bevy_ecs::observer::Observer),
    Resource(StagedResource),
}
```]<registration-enum>

#figure(caption: [Ordered list of plugin items.])[
```rust
struct OrderedPluginItems(Vec<DeclrItem>);
```
]<registration-list>

Two of these Items are simple: `Observer` is a `bevy_ecs` type, and `MessageRegistration` is a pointer to `App::add_message::<T>` where `T` implements `Message`.

`StagedResource`, `StagedSystem`, and `StagedScheduleLabel` each exist for their own reasons. `StagedResource` gets around how `Resource` is not a _dyn-compatible_ trait. `StagedSystem` and `StagedScheduleLabel` simplify the registration process for highly-typed parts of the Bevy API.

== Erased and Staged Values

"Staged" values (see @staging-type) are a pairing between an owned, type erased value and a function that can reverse the type erasure of the value then add that value to `World`. The function pointer has "knowledge" of what the type of the erased value is because it was chosen when type information was still available.

#figure(caption: [A pair of an erased value, and a function that can reverse the erasure process and apply the value to a `World`])[
```rust
// "E" is a newtype over an erased 
// value, specific to each use case. 
// Erased Resources != Erased Systems.
struct Staged<E> {
    // The erased value.
    erased: E,
    // Reverse the erasure.
    apply: fn(&mut World, E),
}
```
]<staging-type>

Type erased values in this context have properties that mean we need to store a lot of extra information about them, extra metadata. I'm calling this a `MetadataPtr` (see @metadata-ptr), but this name doesn't matter much. The properties we need to account for are:

- There could be multiple instances of a plugin/resource of a type, and we will only use one.
- We need to be able to assume the type of the erased value, and visit / consume it if we're right.

Because we will not always end up using the erased value, and the erased value may have data stored on the heap, we need to store a drop function that we run from a `MetadataPtr`'s own Drop implementation. This drop function needs to be able to reverse the erasure of the pointer, rather than just deallocate. We also need to know if the drop function has already been run.

#figure(caption: [A type-erased value with the "Metadata" needed to properly manage it.])[
```rust
struct MetadataPtr {
    layout: Layout,
    ptr: NonNull<()>,
    drop_fn: fn(NonNull<()>, Layout),
    type_id: TypeId,
    already_dropped: bool,
}
```
] <metadata-ptr>

Layout has been stored here largely for debugging reasons during development #footnote()[It is unpleasant to spend an hour asking yourself why a value isn't reversing its erasure properly, only to realise you erased a `Box<Type>` instead of a `Type`.], and can be argued for removal. Everything else is necessary to be able to destroy or visit or reacquire ownership of the value.

Visiting a value also requires that the resulting value cannot contain a reference for the erasure-reversed value. To do this, the visit method has a `visit::<T, A>(self: &MetadataPtr, visitor: impl for<'a> Fn(&'a T) -> A) -> Option<A>` signature, where `T` is the type we're assuming while visiting and `A` is the resulting value.

Reversing erasure and retaking ownership has a more simple signature of `reverse_erasure::<T>(self: MetadataPtr) -> Result<T, MetadataPtr>`, where we use the original `MetadataPtr` as the "error" type to denote that the wrong type was "guessed."

With these mechanisms of type erasure and staging, or relying on existing Bevy/dyn-compatibility functionality, we have the ability to store erased values and insert them into `World` in a way consistent with the well-typed interfaces of Bevy.

== Unsolved Graph <unsolved-h>

Finding a subgraph can be a very expensive operation, and doesn't always express the behavior you want from a program. Consider the naive digraph in @naive-graph2:

#figure(caption:[A naively fully-featured digraph expressing unsolved dependencies between nodes.])[
  #import "diagrams/dependency-graphs.typ": plugins-ungrouped
  #if_html(plugins-ungrouped())
]<naive-graph2>

This expresses dependencies in a way that is fine-grained, but doesn't line up with how dependencies work in something like a build system where you accept that a solver is going to do some of the work for you.

Instead, we put all instances of the same plugin type into a node together and let them depend on whole plugin types, not instances.

An unsolved graph has nodes that store each instance of a plugin of a given type, and edges between each _instance_ to each _plugin type_. That is, $A_1, A_2, A_3 in A$ are all _instances_ of a plugin type `A: Plugin`, and edges are pairs of `(InstanceId, PluginTypeId)` (expressed in @unsolved-graph as $A_n ->_"dep" C$). When expressing dependencies, they can only point towards a plugin type, not a plugin instance.

#figure(caption: [An unsolved graph, with multiple instances of plugins of types $A$ and $C$, and only one instance of $A$ that depends on $B$.])[
  #import "diagrams/dependency-graphs.typ": plugins-grouped
  #if_html(plugins-grouped())
] <unsolved-graph>

This expresses how plugins can register dependencies, and place constraints on those dependencies, but cannot be overly exact without expressing something that might break during plugin resolution.

== Solved Graph

#figure(caption: [A possible solution for the graph in @unsolved-graph. Note that because $A_2$ was picked, there is no edge between $A$ and $B$.])[
  #import "diagrams/dependency-graphs.typ": plugins-solved
  #if_html(plugins-solved())
] <solved-graph>

A Solved Graph is one where we only have one instance of every plugin. It exists after the process of choosing which instance of a plugin to use. The only uncertain thing at this point in the process is which instances of a `Resource` to use, but most of the time resources will only be registered by a single plugin.

The process of choosing a resource is similar to that of choosing a plugin instance, which will be explored later.

== Approval Functions

In #link("https://doc.rust-lang.org/cargo/reference/resolver.html#semver-compatibility")[Cargo], and other modern package managers, versions of dependencies are usually chosen through some form of SAT solve on the version numbers of the dependencies.

Plugin configurations in Bevy are structs and enums. There isn't a simple domain specific language we can use to determine which configuration for a plugin is appropriate. Instead we use "Approval" functions, which for a plugin type $A$ is just a function of $\&A -> bool$ that can account for the type erasure in play.

Unlike with versioning, which cargo solves for us already, we only rarely need to care about plugin configuration. It's not expected that most plugins use approvals.

== Entry Points

"Entry Points" are the plugins that are added at the root of the dependency graph, by an app developer. These are the arguments to the function `DeclarativePlugin::add_plugins_declarative`. Because the app developer is putting these in manually, we treat them as "most important" during the graph solving algorithms as to best respect the intent of these developers.

```rs
fn main() {
    App::new()
        // A and B are entry points.
        .add_declarative_plugins((A, B))
}

impl DeclarativePlugin for A {
    fn build(&self, output: &mut PluginOutput) {
        // C is not an entry point.
        output.add_dependencies(C)
    }
}
```

== `PluginOutput`

How do we store all the information we need from a plugin to begin with? With a big struct that stores all the erased data in individual containers alongside some extra information for solving the graph:

#figure(caption: [The datatype that is modified by `DeclarativePlugins::build`, in lieu of `&mut App`.])[
```rs
// Users should not be given access to any of this, presented with an additive
// API only with no inspection of what the app has already added.
struct PluginOutput<D = PluginDependencies> {
    // Solving-relevant information.
    is_entry_point: bool,
    is_zero_sized_optimizable: bool,
    working_plugin: PluginTypeId,
    plugin_name: &'static str,
    
    // Dependencies
    dependencies: D,

    // Items and Approvals
    observers: Vec<Observer>,
    schedules: StagedSchedules,
    schedule_labels: Vec<StagedScheduleLabel>,
    messages: HashMap<TypeId, MessageRegistration>,
    resource_staging: HashMap<ResourceTypeId, StagedResource>,
    plugin_approval: HashMap<PluginTypeId, Approval<Box<dyn DeclarativePlugin>>>,
}
```
]

An important part of this data structure is that the fields for these are *_private_* and not the basis of the public API. Another is that the public API is *_additive-only_*, there are no methods exposed on this data structure that a plugin author or app developer can use to _remove_ something.

Other than information necessary for our graph solving and the erased storage fields, the dependencies of a `PluginOutput<D>` are a generic parameter. This is for two reasons: so that we can mark dependencies as "used" when we've populated the unsolved graph, and so we can define subapp registration functionality, such as with `RenderApp`. If dependencies are `()`, then you can't register dependencies and we don't have to worry about multiple graphs, just items that are meant for a different `App`/`World`.

= Algorithms

== Ordering of Bevy Items

- Systems.
- Observers.
- Resources (states get wrapped up in this).
- Schedule Labels.
- Event types (Messages and Entity Events. This amounts to type registration.)
- Other plugins ("Dependencies.")


1. Other Plugins (We want everything we depend on to be present first.)
2. Messages / Events (Type registration, Zero Dependencies.)
3. States (Are a resource.)
4. Schedule Labels (Zero Dependencies but might want to depend on States.)
5. Observers (Hypothetically wants to know Events.)
6. Systems (Wants to know Schedule Labels.)
7. Resources (Wants to know Observers due to Resources As Entities.)

(1) leaves us needing to recursively perform (2-7) for all prior plugins as part of the resolution tree.

(2-3) can be done all at once, without any ordering applied.

(4) can be done across all plugins, but there are dependencies between them.

(5, 7) can be done across all plugins after all event types are registered (3).

(6) can be done across all plugins after all schedule labels are registered (4)


== Unsolved Graph Construction Algorithm

Constructing the unsolved graph has its own nuances we want to consider. The algorithm described in @graph-construction is a breadth-first expansion, as we want to prioritise the instances we find closest to the Entry Points a developer specifies. The size of the graph is constrained by a program only having a finite number of types as well as a limit on the number of instances of a specific plugin that can occur in the graph.

As shown in @unsolved-h, the shape of an unsolved graph is that where nodes are comprised of many plugin instances, and edges are from instance to node to represent the "lossy" nature of depending on something.

For zero-sized types, which comprise most plugin types in practice, we can make some assumptions that simplify the expansion process. These are accounted for in @graph-construction.

If we come across a plugin config that fails approval by anything that depends on it, then we can decide to avoid evaluating it at all. We eagerly remove these, so even if one instance of a plugin disapproves of it and the rest of those plugins don't then the instance that is being evaluated will be discarded.

#figure(caption: [Breadth-first Graph Construction Algorithm with Proactive Culling.])[
  // #set edge(crossing: true)
  #import "diagrams/algo-expansion.typ": algo-expansion
  #if_html(algo-expansion())
]<graph-construction>

=== State Machine Behaviour and Plugin Instance Limits <infinite-h>

We can construct a plugin approval function for two plugins that, left unchecked, would consume all resources of a machine it runs on:

1. A plugin `struct A(usize)`, shown here as $A(n)$ and its inner value as $A_0$
2. A plugin `struct B(usize)`, shown here as $B(m)$ and its inner value as $B_0$
3. $A(n)$ requires $B(n + 1)$, approves $B$ only if $B_0 > n$
4. $B(m)$ requires $A(m + 1)$, approves $A$ only if $A_0 > m$

We can see this expansion happen in @inf, where every existing instance of $A$ or $B$ do not meet the approval functions registered by either.

#figure(caption: [Infinitely expanding dependency, where $A(0)$ is the entry point.])[
  #diagram(
    node-stroke: .1em,
    spacing: 1em,
    node-shape: circle,
    node((-2, 0), [$A(0)$], stroke: (dash: "dashed"), name: <A0>),
    node((-1, 0), [$B(1)$], name: <B1>),
    node((0, 0), [$A(2)$], name: <A2>),
    node((1, 0), [$B(3)$], name: <B3>),
    node((2, 0), [$A(4)$], name: <A4>),
    node((3, 0), [...], name: <inf>, stroke: (paint: white.transparentize(100%))),
    edge(<A0>, <B1>, "-|>"),
    edge(<B1>, <A2>, "-|>"),
    edge(<A2>, <B3>, "-|>"),
    edge(<B3>, <A4>, "-|>"),
    edge(<A4>, <inf>, "--|>"),
    edge(<B1>, <A0>, bend: 90deg, "-|>", stroke: (paint: red)),
    edge(<A2>, <B1>, bend: -90deg, "-|>", stroke: (paint: red)),
    edge(<B3>, <A0>, bend: 90deg, "-|>", stroke: (paint: red)),
    edge(<B3>, <A2>, bend: 90deg, "-|>", stroke: (paint: red)),
    edge(<B1>, <A0>, bend: 90deg, "-|>", stroke: (paint: red)),
    edge(<A4>, <B1>, bend: -90deg, "-|>", stroke: (paint: red)),
  )
] <inf>

The issue here is that all smaller instances will approve of all larger instances, but all larger instances disapprove of all smaller instances.

We avoid this consuming all resources by having a limit for the number of instances of a single plugin. The breadth-first approach to expansion lets us, in the case of a less-infinite version of this, bias towards instances closer to the app developer's intent.

In the case of infinite expansion, detecting repeating sub-cycles in a graph may be more desirable from a point of soundness. This isn't practical for people whose main concern is building and shipping a game or app. Having a low ceiling on how many instances of a plugin can be registered before the resolution process returns an error makes more sense for keeping startup times in player/user-friendly timescales and keeping errors interpretable to developers.

=== Zero-Sized Plugin Types

Most plugin types are _Zero-Sized Types_ (ZSTs) in that they are a struct of the form in @zst-fig, equivalent to `()`.

#figure(caption: [A Zero-Sized Type.])[
```rust
pub struct MyFeaturePlugin;
```
] <zst-fig>

A ZST only has one valid state. This allows us to make the following assumptions during graph expansion:

1. Any two instances of a ZST are the same semantically. $A = A$.
2. The output of any two instances of a ZST are the same.

(1) holds when the ZST implements `'static`, as `Any` has been introduced as a supertrait for plugins this holds for all plugins. 

(2) can be violated by users running code dependent on information available to the machine it is running on (i.e. random information, clock time). We can weaken this into the assertion "We will only use the first output from a ZST plugin," so that even if a plugin does violate the strong form of (2) it isn't relevant to the graph expansion process.

== Solving the Graph

Graph construction was conducted in such a way where the graph solve is relatively simple:

1. Apply all approval functions to all instances.
2. Pick the instances that passed approval that are closest to the entry points, if not entry points themselves.

=== Applying Approval Functions <approvalh>

#figure(caption:[Expansion of the conflict in @plugin-conflict, showing the results of the approval functions.])[
  #import "diagrams/dependency-graphs.typ": conflict
  #if_html(conflict())
] <conflict-expansion>

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

=== Choosing Between Instances

#figure(caption: [Entry point distances.])[
  #import "diagrams/dependency-graphs.typ": distance
  #if_html(distance())
]

=== Conflicting Requirements

#figure(caption: [A plugin dependency graph with a conflict between two non-_ZST_ plugins.])[
  #import "diagrams/dependency-graphs.typ": conflict-req
  #if_html(conflict-req())
]<plugin-conflict>


== Choosing Resources

== Plugins that touch the Rendering World

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
