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