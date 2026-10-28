#import "@preview/fletcher:0.5.8": diagram, edge, node, as-label
#import "../shared.typ": *
#let algo-expansion() = {
 diagram(
    node-stroke: .1em,
    spacing: 2em,
    node-shape: rect,
    node((0,0), [Populate Queue with Entry Point Plugins], name: <populate>),
    node((0,1), [Pop front of queue], name: <pop>),
    node((-0.5,2), `Some(StagedPlugin)`, name: <some>),
    node((0.5,2), `None`, name: <none>),
    node((0.5,3), [Finished building Graph], name: <finish>),
    node((-0.5,3), [Is a _ZST_?], name: <iszst>),
    node((-0,4), [Check Approval], name: <veto>),
    node((-1,4), [Check if\ already present], name: <zstpresent>),
    node((-0,6), [Compute $output$], name: <getoutput>),
    node((-0,7), [Extract Dependencies into\ ($output',dependencies)$], name: <extractdeps>),
    node((0,8), [Add $output'$ to Graph\ Push $dependencies$ to back of queue], name: <addpush>),
    edge(<populate>, <pop>, "-|>"),
    edge(<pop>, <some>, "-|>"),
    edge(<pop>, <none>, "-|>"),
    edge(<none>, <finish>, "-|>"),
    edge(<some>, <iszst>, "-|>"),
    edge(<iszst>, <veto>, label: "No", "-|>"),
    edge(<iszst>, <zstpresent.east>, label: "Yes", "-|>"),
    edge(<zstpresent>, (-1, 6), <getoutput>, label-pos: 80%, label: "Not present", "-|>"),
    edge(<zstpresent.west>, (-2, 4), "|>--"),
    edge(<veto.west>, (-0.5, 4), (-0.5,5), (-2, 5), label-pos: 20%, crossing: true, "|>--"),
    edge(<veto>, <getoutput>, label: "Approved", "-|>"),
    edge(<getoutput>, <extractdeps>, "-|>"),
    edge(<extractdeps>, <addpush>, "-|>"),
    edge(<addpush>, (-2, 8), (-2, 1), <pop>, "|>--|>"),
  ) 
}