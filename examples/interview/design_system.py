"""Entry point: builds the whole `Design System` page of the Interview project (idempotent).

    python3 design_system.py

Foundations (ds_foundations.py) -> main components (ds_components.py, top-level frames, laid out in the
library area right of the sections) -> Section / Components gallery (instances with captions).
"""
import ds_foundations
import ds_components as C
from ds_common import *

X_LIB = 1560


def gallery(P):
    body = ""
    for g, names in C.GROUPS.items():
        cells = ""
        for n in names:
            cells += col(f"Cell / {n}", T("Caption", n, 12, "var(--color-text-secondary)", lh=16) + inst_html(n), gap="8px")
        body += col(f"Group / {g}", T("Group Title", g, 20, weight=600) + row("Cells", cells, flex_wrap="wrap", gap="var(--spacing-lg)", align_items="flex-start"),
                    gap="var(--spacing-md)", padding="var(--spacing-lg)", background="var(--color-bg-layout)", border_radius="var(--radius-lg)", width="1376px")
    html = col("Section / Components", T("Section Title", "Components", 24, weight=700) +
               T("Section Intro", "Every item is an instance of a main component; main components live in the library area to the right. Use the exact names in <x-paper-instance component=...>.", 14, "var(--color-text-secondary)") +
               col("Groups", body, gap="var(--spacing-xl)"), width="1440px", gap="var(--spacing-xl)", padding="var(--spacing-xl)", background="transparent")
    return P.add("root", html)


def stack(P, ids):
    y = 0
    for i in ids:
        P.c("update_styles", updates=[{"nodeIds": [i], "styles": {"left": 0, "top": y}}])
        h = next(c["height"] for c in P.c("get_children", nodeId="root")["children"] if c["id"] == i)
        y += h + 48
    return y


if __name__ == "__main__":
    fid, P = setup()
    clear(P)
    vals = call("get_variables", {"fileId": fid})["variables"]
    f_ids = ds_foundations.build(P, vals)
    ids, ylib = place_components(P, C.SPECS, X_LIB, 0, 2800, C.GROUPS)
    g_ids = gallery(P)
    stack(P, f_ids + g_ids)
    print("components:", len(ids))
