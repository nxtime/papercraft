# UI kits and code links

## Why

The Code & AI Handoff rebuilds every layer as `<div>`s with Tailwind classes, and the AI spec
describes boxes. A team that ships with shadcn/ui, Ant Design or React Bits then re-maps each
box to its library by hand, and the design never looked like that library in the first place.
For egui the same holds: the export draws frames where the app would call `ui.button`.

## What

1. **Code links** (like Figma's Code Connect): a main component can carry a `CodeLink`: the
   kit, the import line, and a usage template per target, React (`<Button variant="outline">{Label}</Button>`)
   and egui (`ui.button({Label})`). `{Name}` in a template is the text of the layer named
   `Name` in the instance (its override when there is one). Set with the `set_code_link` tool
   or by a kit.
2. **Kits** (data, `src/ui_kits.rs`): shadcn/ui, Ant Design, React Bits (animated text,
   backgrounds and a dock, on top of any styling) and egui. Each kit has its tokens (colors in
   light and dark, radius, control height, font size) and a set of components: the markup that
   draws it in that library's look and density, and its code link. `apply_ui_kit` builds a
   "UI Kit / <name>" page with the variables, tokens and linked components, ready to place
   with `<x-paper-instance>`.
3. **Handoff**: React and Tailwind exports emit a linked instance as its template (with the
   imports gathered at the top) instead of its layers; the egui export does the same with the
   egui template. The AI spec gets a "Library components" section (component, import, usage,
   where it is used) and the rule to use them, not to rebuild them.
4. **Chat**: the brief asks which library the code will use; the agent applies that kit
   before building, so screens are made of linked components from the start.

## Not now

Variant props beyond text (a variant is its own component with its own template), Vue and
Svelte templates, importing a kit from a package registry.

## Acceptance

* A linked Button instance with the override `Label: "Save"` exports in React as
  `<Button variant="default">Save</Button>` with `import { Button } from "@/components/ui/button"`
  once at the top, and in egui as its egui call.
* An unlinked frame exports as before.
* `apply_ui_kit({kit:"shadcn"})` makes a page with the kit's variables and its components, each
  a main component with a code link.
* The AI spec of a screen with linked instances lists them with import and usage.
