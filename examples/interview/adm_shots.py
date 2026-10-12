"""Screenshot Admin screens: python3 adm_shots.py <section name> [screen name substr ...]"""
import sys; sys.path.insert(0, "/Volumes/Storage/Projects/papercraft/examples/interview")
from adm_common import *
OUT = "/private/tmp/claude-501/-Volumes-Storage-Projects-photocraft/11f4040a-b4f4-4b9e-965a-bde08b400450/scratchpad/"
fid, A = setup()
sec = next(c for c in A.c("get_children", nodeId="root")["children"] if c["name"] == sys.argv[1])
for slot in A.c("get_children", nodeId=sec["id"])["children"]:
    if not slot["name"].startswith("Screen Slot"): continue
    n = slot["name"][len("Screen Slot / "):]
    if len(sys.argv) > 2 and not any(a.lower() in n.lower() for a in sys.argv[2:]): continue
    scr = next(c for c in A.c("get_children", nodeId=slot["id"])["children"] if c["name"].startswith("Screen /"))
    p = OUT + "adm_" + n.replace(" ", "_") + ".png"
    tmp = A.artboard("TMP shot", "", width="1440px", height="900px", left="-3000px", top="0px")
    A.c("write_html", targetNodeId=tmp, mode="insert-children", html=f'<x-paper-clone node-id="{scr["id"]}"/>')
    A.screenshot(p, tmp, 1); print(p, scr["width"], scr["height"])
    A.c("delete_nodes", nodeIds=[tmp])
    r = A.lint(scr["id"])
    iss = r.get("issues") or r.get("findings") or r
    errs = [i for i in (iss if isinstance(iss, list) else []) if i.get("severity") in ("error", "warning")]
    print("  lint", len(errs), {(i.get("rule") or i.get("code")) for i in errs})
