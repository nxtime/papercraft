"""Screenshot each Candidate Challenge screen: python3 cand_challenge_shots.py [filter]"""
import sys
from cand_common import *
fid, P, DS = setup_cand()
def walk(nid, out):
    for c in P.c("get_children", nodeId=nid)["children"]:
        if c["name"].startswith("Screen / Challenge"): out[c["name"]] = c["id"]
        elif c["name"].startswith(("Section /", "Screens", "Row ", "Block /")): walk(c["id"], out)
    return out
sec = next(c["id"] for c in top_children(P) if c["name"] == "Section / Candidate Challenge")
for n, i in walk(sec, {}).items():
    if len(sys.argv) < 2 or sys.argv[1] in n:
        print(shot(P, i, n.replace(" / ", "_").replace(" ", "_")))
