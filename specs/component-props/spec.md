# Component properties

## Why

A status badge, a tab bar or a list row is drawn once as a component, but every use differs:
the status is Autorizada, Rejeitada or Na SEFAZ; a different tab is active; the label and the
icon change. Today each case is its own component (`Badge / Green`, `Tabs / Lojas`, `Tabs /
Pedidos`) or hand edits on layers, so a design needs a component per state and an agent edits
layer by layer.

## What

1. **Properties live on the main component** (`Node.props`), each with a name and a type:
   * `text`: the text of a layer (`layer`).
   * `bool`: shows or hides a layer.
   * `icon`: the icon of an icon layer (`Icon / set:name`), any `set:name`.
   * `enum`: a list of options; each option is a set of changes in the `overrides` format
     (`{"Label": "Rejeitada", "$root": {"background": "var(--danger-bg)"}, "Dot": {"icon": "lucide:x"}}`),
     so an option can restyle, retext, show, hide or re-icon any layer. Fully custom.
2. **Instances keep their values** (`Node.prop_values` on the instance root) and apply them as
   overrides, so edits to the main keep them; editing the definitions re-applies every
   instance's values on every page.
3. **Templates** make the common cases one call: `text` (a text property per text layer),
   `active` (an enum choosing which item of a row, tab bar or nav is active, with active and
   inactive styles) and `status` (an enum of statuses, each a colour that tints the root and
   colours its text and icon).
4. **Where to use it**: `set_component_props` (define, or from a template),
   `set_instance_props` (many instances in one call), `get_components` (lists properties),
   `<x-paper-instance component="Badge" props='{"Status":"Rejeitada"}'/>` in `write_html`,
   the inspector (a control per property on an instance; templates on a main), and code
   links (`{Status}` in a usage template is the instance's value).

## Not now

Variant sets made of several mains, nested property exposure (a row exposing its badge's
properties), numeric properties.

## Acceptance

* A badge with an enum Status (Autorizada, Rejeitada) shows the option's colours and label on
  each instance; switching the value updates only that instance.
* Editing the main (gap) keeps every instance's values; editing an option's colour updates
  every instance using it.
* The `active` template on a five-item tab bar makes one property whose options are the item
  names; each instance shows its active item highlighted.
* An icon property swaps the icon and survives a sync of the main.
