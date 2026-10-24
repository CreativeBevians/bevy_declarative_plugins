#import "@preview/fletcher:0.5.8": diagram, edge, node, as-label
#import "../shared.typ": *

#let current-plugin-order() = {diagram(
  node-stroke: .1em,
  spacing: 1em,
  node((0,0), plugins.framework, name: <Def>),
  node((1,1), plugins.windowing, name: <Win>),
  node((1,2), plugins.graphics, name: <Gra>),
  node((1,3), plugins.audio, name: <Aud>),
  node((0,4), plugins.user.a, name: <A>),
  node((1,5), plugins.user.input, name: <Imp>),
  node((0,6), plugins.user.b, name: <B>),
  node((1,7), plugins.user.game_systems, name: <Gam>),
  node((2,8), plugins.user.ui, name: <UI>),
  edge(<Def>, (0,1), <Win>, "-|>"),
  edge(<Def>, (0,2), <Gra>, "-|>"),
  edge(<Def>, (0,3), <Aud>, "-|>"),
  edge(<Def>, <A>, "-|>"),
  edge(<A>, (0,5), <Imp>, "-|>"),
  edge(<A>, <B>, "-|>"),
  edge(<B>, (0,7), <Gam>, "-|>"),
  edge(<Gam>, (1,8),<UI>, "-|>"),
)}