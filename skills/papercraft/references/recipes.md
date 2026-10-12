# Recipes

Every snippet here was run against PaperCraft and passes `lint_design` with no errors or warnings. They assume the variables from recipe 0 and `jq` on the PATH. Wrap HTML files into a `write_html` call with:

```sh
jq -Rs --arg t "$ARTBOARD_ID" '{targetNodeId:$t,mode:"insert-children",html:.}' file.html | papercraft-cli call write_html -
```

## 0. Tokens first (do this once per project)

Two modes, so one call can flip the whole design between dark and light. Roles, not hues.

```sh
papercraft-cli call set_variables '{"mode":"Dark","variables":{"bg":"#1e1e2e","surface":"#313244","border":"#45475a","text":"#cdd6f4","text-muted":"#a6adc8","accent":"#cba6f7","on-accent":"#11111b","danger":"#f38ba8"}}'
papercraft-cli call set_variables '{"mode":"Light","variables":{"bg":"#eff1f5","surface":"#e6e9ef","border":"#bcc0cc","text":"#4c4f69","text-muted":"#5c5f77","accent":"#8839ef","on-accent":"#eff1f5","danger":"#d20f39"}}'
papercraft-cli call set_variable_mode '{"mode":"Dark"}'
```

The first mode you define stays active until you switch. Check contrast in both modes with `lint_design` after each switch.

## 1. Mobile app flow

Build the parts once as components, then compose screens from instances.

```sh
# a. The button: the artboard IS the component (44px tall: 12 + 20 line + 12)
BTN=$(papercraft-cli call create_artboard '{"name":"Button / Primary","styles":{"width":"fit-content","height":"fit-content","display":"flex","backgroundColor":"var(--accent)","borderRadius":"12px","padding":"12px 24px","justifyContent":"center","alignItems":"center"}}' | jq -r .id)
echo '{"targetNodeId":"'$BTN'","mode":"insert-children","html":"<div layer-name=\"Label\" style=\"font-size:16px;font-weight:600;line-height:20px;color:var(--on-accent)\">Button</div>"}' | papercraft-cli call write_html -
papercraft-cli call create_component '{"nodeId":"'$BTN'"}'

# b. The screen: 390x844, content centered, 24px padding
SCREEN=$(papercraft-cli call create_artboard '{"name":"Sign in","styles":{"width":"390px","height":"844px","backgroundColor":"var(--bg)","padding":"24px","gap":"16px","justifyContent":"center"}}' | jq -r .id)
cat > /tmp/signin.html <<'HTML'
<div layer-name="Header" style="display:flex;flex-direction:column;gap:8px">
  <div layer-name="Title" style="font-size:32px;font-weight:700;color:var(--text)">Welcome back</div>
  <div layer-name="Subtitle" style="font-size:16px;line-height:24px;color:var(--text-muted)">Sign in to sync your tasks.</div>
</div>
<div layer-name="Field / Email" style="display:flex;align-items:center;gap:12px;padding:12px 16px;background:var(--surface);border:1px solid var(--border);border-radius:12px">
  <x-paper-icon name="phosphor:envelope-simple" size="20" color="var(--text-muted)"/>
  <div layer-name="Value" style="font-size:16px;color:var(--text)">marcos@example.com</div>
</div>
<div layer-name="Actions" style="display:flex;flex-direction:column;gap:12px"></div>
HTML
jq -Rs --arg t "$SCREEN" '{targetNodeId:$t,mode:"insert-children",html:.}' /tmp/signin.html | papercraft-cli call write_html -

# c. Place the instance in Actions (find its id with get_children), then set its text and width
ACTIONS=$(papercraft-cli call get_children '{"nodeId":"'$SCREEN'"}' | jq -r '.children[]|select(.name=="Actions").id')
INST=$(papercraft-cli call create_instance '{"componentId":"'$BTN'","parentId":"'$ACTIONS'","name":"Button / Sign in"}')
LABEL=$(echo "$INST" | jq -r '.textLayers[0].id'); ID=$(echo "$INST" | jq -r .id)
papercraft-cli call set_text_content '{"updates":[{"nodeId":"'$LABEL'","textContent":"Sign in"}]}'
papercraft-cli call update_styles '{"updates":[{"nodeIds":["'$ID'"],"styles":{"width":"342px"}}]}'
papercraft-cli call lint_design '{"nodeId":"'$SCREEN'"}'
papercraft-cli call get_screenshot '{"nodeId":"'$SCREEN'"}' --out signin.png
```

For the next screens, create each artboard at 390x844, reuse the components by `create_instance`, and give every screen a status bar and a tab bar component so they stay identical. Name screens as nouns (`Sign in`, `Today`, `Task detail`). Put each flow's screens left to right: a new artboard goes to an empty spot, or set `left` and `top` in `styles`. `examples/taskflow.py` in the PaperCraft repository is a complete 8-screen reference.

## 2. Landing page (1440 wide, height grows with content)

Create the artboard with `height:"fit-content"` so it never clips, then add one section per call.

```sh
AB=$(papercraft-cli call create_artboard '{"name":"Landing","styles":{"width":"1440px","height":"fit-content","backgroundColor":"var(--bg)","display":"flex","flexDirection":"column","paddingBottom":"96px"}}' | jq -r .id)
jq -Rs --arg t "$AB" '{targetNodeId:$t,mode:"insert-children",html:.}' landing.html | papercraft-cli call write_html -
```

`landing.html` (nav, hero with one primary action, three feature cards):

```html
<div layer-name="Nav" style="display:flex;justify-content:space-between;align-items:center;padding:24px 64px">
  <div layer-name="Logo" style="font-size:20px;font-weight:700;color:var(--text)">Acme</div>
  <div layer-name="Links" style="display:flex;align-items:center;gap:32px">
    <div layer-name="Link / Product" style="font-size:16px;color:var(--text-muted)">Product</div>
    <div layer-name="Link / Pricing" style="font-size:16px;color:var(--text-muted)">Pricing</div>
    <div layer-name="Button / Nav" style="display:flex;padding:12px 24px;background:var(--accent);border-radius:12px"><div layer-name="Label" style="font-size:16px;font-weight:600;line-height:20px;color:var(--on-accent)">Sign up</div></div>
  </div>
</div>
<div layer-name="Hero" style="display:flex;flex-direction:column;align-items:center;gap:24px;padding:96px 64px">
  <div layer-name="Eyebrow" style="font-size:14px;font-weight:600;letter-spacing:1px;color:var(--accent)">NOW IN BETA</div>
  <div layer-name="Headline" style="font-size:64px;font-weight:800;line-height:72px;letter-spacing:-2px;text-align:center;color:var(--text);width:960px">Plan the day. Finish what matters.</div>
  <div layer-name="Subhead" style="font-size:20px;line-height:32px;text-align:center;color:var(--text-muted);width:640px">One calm list for everything on your plate, synced across your devices.</div>
  <div layer-name="Actions" style="display:flex;gap:16px">
    <div layer-name="Button / Primary" style="display:flex;padding:16px 32px;background:var(--accent);border-radius:12px"><div layer-name="Label" style="font-size:16px;font-weight:600;line-height:20px;color:var(--on-accent)">Get started</div></div>
    <div layer-name="Button / Secondary" style="display:flex;padding:16px 32px;border:1px solid var(--border);border-radius:12px"><div layer-name="Label" style="font-size:16px;font-weight:600;line-height:20px;color:var(--text)">See how it works</div></div>
  </div>
</div>
<div layer-name="Features" style="display:flex;gap:24px;padding:0 64px">
  <div layer-name="Feature Card / Capture" style="display:flex;flex-direction:column;gap:16px;padding:32px;background:var(--surface);border-radius:16px;flex-grow:1"><x-paper-icon name="phosphor:lightning" size="32" color="var(--accent)"/><div layer-name="Title" style="font-size:20px;font-weight:600;color:var(--text)">Capture fast</div><div layer-name="Body" style="font-size:16px;line-height:24px;color:var(--text-muted)">Type a task and move on.</div></div>
  <div layer-name="Feature Card / Focus" style="display:flex;flex-direction:column;gap:16px;padding:32px;background:var(--surface);border-radius:16px;flex-grow:1"><x-paper-icon name="phosphor:target" size="32" color="var(--accent)"/><div layer-name="Title" style="font-size:20px;font-weight:600;color:var(--text)">Stay focused</div><div layer-name="Body" style="font-size:16px;line-height:24px;color:var(--text-muted)">See only what is due today.</div></div>
  <div layer-name="Feature Card / Sync" style="display:flex;flex-direction:column;gap:16px;padding:32px;background:var(--surface);border-radius:16px;flex-grow:1"><x-paper-icon name="phosphor:arrows-clockwise" size="32" color="var(--accent)"/><div layer-name="Title" style="font-size:20px;font-weight:600;color:var(--text)">Always in sync</div><div layer-name="Body" style="font-size:16px;line-height:24px;color:var(--text-muted)">Phone, tablet and desktop agree.</div></div>
</div>
```

Check the headline in the screenshot: text boxes with a fixed `width` can wrap earlier than in a browser, so give long headlines generous widths (960px above) and verify.

## 3. Dashboard (sidebar, stat cards, chart)

A row artboard (`flexDirection:"row"`) with a fixed 240px sidebar and a `flex-grow:1` main area.

```sh
AB=$(papercraft-cli call create_artboard '{"name":"Dashboard","styles":{"width":"1440px","height":"900px","backgroundColor":"var(--bg)","display":"flex","flexDirection":"row"}}' | jq -r .id)
jq -Rs --arg t "$AB" '{targetNodeId:$t,mode:"insert-children",html:.}' dashboard.html | papercraft-cli call write_html -
```

`dashboard.html`:

```html
<div layer-name="Sidebar" style="display:flex;flex-direction:column;gap:8px;padding:24px 16px;width:240px;background:var(--surface)">
  <div layer-name="Brand" style="font-size:20px;font-weight:700;color:var(--text);padding:8px 12px">Acme</div>
  <div layer-name="Nav / Overview" style="display:flex;align-items:center;gap:12px;padding:12px;border-radius:12px;background:var(--accent)"><x-paper-icon name="phosphor:squares-four" size="20" color="var(--on-accent)"/><div layer-name="Label" style="font-size:16px;font-weight:600;color:var(--on-accent)">Overview</div></div>
  <div layer-name="Nav / Customers" style="display:flex;align-items:center;gap:12px;padding:12px;border-radius:12px"><x-paper-icon name="phosphor:users" size="20" color="var(--text-muted)"/><div layer-name="Label" style="font-size:16px;color:var(--text-muted)">Customers</div></div>
  <div layer-name="Nav / Reports" style="display:flex;align-items:center;gap:12px;padding:12px;border-radius:12px"><x-paper-icon name="phosphor:chart-line-up" size="20" color="var(--text-muted)"/><div layer-name="Label" style="font-size:16px;color:var(--text-muted)">Reports</div></div>
</div>
<div layer-name="Main" style="display:flex;flex-direction:column;gap:24px;padding:32px;flex-grow:1">
  <div layer-name="Title" style="font-size:32px;font-weight:700;color:var(--text)">Overview</div>
  <div layer-name="Stat Row" style="display:flex;gap:16px">
    <div layer-name="Stat Card / Revenue" style="display:flex;flex-direction:column;gap:8px;padding:24px;background:var(--surface);border-radius:16px;flex-grow:1"><div layer-name="Label" style="font-size:14px;color:var(--text-muted)">Revenue</div><div layer-name="Value" style="font-size:32px;font-weight:700;color:var(--text)">$48,200</div></div>
    <div layer-name="Stat Card / Orders" style="display:flex;flex-direction:column;gap:8px;padding:24px;background:var(--surface);border-radius:16px;flex-grow:1"><div layer-name="Label" style="font-size:14px;color:var(--text-muted)">Orders</div><div layer-name="Value" style="font-size:32px;font-weight:700;color:var(--text)">1,204</div></div>
    <div layer-name="Stat Card / Refunds" style="display:flex;flex-direction:column;gap:8px;padding:24px;background:var(--surface);border-radius:16px;flex-grow:1"><div layer-name="Label" style="font-size:14px;color:var(--text-muted)">Refunds</div><div layer-name="Value" style="font-size:32px;font-weight:700;color:var(--danger)">18</div></div>
  </div>
  <div layer-name="Chart Card" style="display:flex;flex-direction:column;gap:16px;padding:24px;background:var(--surface);border-radius:16px;flex-grow:1">
    <div layer-name="Heading" style="font-size:20px;font-weight:600;color:var(--text)">Revenue, Monday to Thursday</div>
    <div layer-name="Bars" style="display:flex;align-items:flex-end;gap:16px;flex-grow:1">
      <div layer-name="Bar / Mon" style="width:48px;height:96px;background:var(--accent);border-radius:8px"></div>
      <div layer-name="Bar / Tue" style="width:48px;height:144px;background:var(--accent);border-radius:8px"></div>
      <div layer-name="Bar / Wed" style="width:48px;height:120px;background:var(--accent);border-radius:8px"></div>
      <div layer-name="Bar / Thu" style="width:48px;height:208px;background:var(--accent);border-radius:8px"></div>
    </div>
  </div>
</div>
```

Charts are just boxes: size the bars from the data (`height = value / max * plot height`). Add axis labels at 12px. Always label what the bars mean; color alone is not information.

## 4. Design-system page

Generate swatches from the project's own variables and a type-scale specimen, so the page cannot drift from the tokens:

```sh
AB=$(papercraft-cli call create_artboard '{"name":"Design system","styles":{"width":"720px","height":"fit-content","backgroundColor":"var(--bg)","display":"flex","flexDirection":"column","padding":"32px","gap":"32px"}}' | jq -r .id)

# Colors: one swatch per variable, built from the project's own variables
SWATCHES=$(papercraft-cli call get_variables | jq -r '.variables | keys[]' | while read -r n; do
  printf '<div layer-name="Swatch / %s" style="display:flex;align-items:center;gap:12px;width:208px"><div layer-name="Chip" style="width:40px;height:40px;border-radius:8px;background:var(--%s);border:1px solid var(--border)"></div><div layer-name="Name" style="font-size:14px;color:var(--text)">--%s</div></div>' "$n" "$n" "$n"
done)
jq -n --arg t "$AB" --arg s "$SWATCHES" '{targetNodeId:$t,mode:"insert-children",html:("<div layer-name=\"Colors\" style=\"display:flex;flex-direction:column;gap:16px\"><div layer-name=\"Heading\" style=\"font-size:24px;font-weight:700;color:var(--text)\">Colors</div><div layer-name=\"Swatches\" style=\"display:flex;flex-wrap:wrap;gap:16px\">"+$s+"</div></div>")}' | papercraft-cli call write_html -

# Type scale: one row per size
ROWS=""; for s in 12 14 16 20 24 32 40; do
  ROWS+="<div layer-name=\"Type / ${s}\" style=\"display:flex;align-items:baseline;gap:16px\"><div layer-name=\"Size\" style=\"font-size:12px;color:var(--text-muted);width:48px\">${s}px</div><div layer-name=\"Sample\" style=\"font-size:${s}px;color:var(--text)\">The quick brown fox</div></div>"
done
jq -n --arg t "$AB" --arg r "$ROWS" '{targetNodeId:$t,mode:"insert-children",html:("<div layer-name=\"Type scale\" style=\"display:flex;flex-direction:column;gap:12px\"><div layer-name=\"Heading\" style=\"font-size:24px;font-weight:700;color:var(--text)\">Type scale</div>"+$r+"</div>")}' | papercraft-cli call write_html -
```

Add component specimens by placing instances of every component on the same page, in a row per component with each state (default, hover, disabled, selected, error) next to it. Keep this page when you finish: it is the contract for later edits.
