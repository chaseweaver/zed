import { ColorScheme } from "../theme/colorScheme"
import { withOpacity } from "../theme/color"
import { text, background } from "./components"
import { toggleable } from "./toggle"
import { interactive } from "./interactive"

export default function commandPalette(colorScheme: ColorScheme) {
  let layer = colorScheme.highest
  return {
    keystrokeSpacing: 8,
    key:
      toggleable(interactive({
        text: text(layer, "mono", "variant", "default", { size: "xs" }),
        cornerRadius: 2,
        background: background(layer, "on"),
        padding: {
          top: 1,
          bottom: 1,
          left: 6,
          right: 6,
        },
        margin: {
          top: 1,
          bottom: 1,
          left: 2,
        },
      }, { hover: { cornerRadius: 4, padding: { top: 17 } } }), {
        default: {
          text: text(layer, "mono", "on", "default", { size: "xs" }),
          background: withOpacity(background(layer, "on"), 0.2),
        }

      })
    ,

  }
}
