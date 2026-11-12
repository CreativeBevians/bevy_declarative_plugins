#import "@preview/fletcher:0.5.8": diagram, edge, node, as-label

= Plugins Overhaul

== Status Quo

Plugin registration is currently a user-specified sequences of function calls on `&mut App`. This kind of access is a double-edged sword, in that a plugin can do _anything_ but we cannot make assumptions about what a plugin does.

== Motivations

== Data structures

== Algorithms