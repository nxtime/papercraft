"""Build the Candidate page of the Interview project (idempotent): components, then the three sections.

    python3 candidate.py [components] [access] [session] [challenge]    (no args = everything)
"""
import sys

sys.path.insert(0, "/Volumes/Storage/Projects/papercraft/examples/interview")
from cand_common import *

LIB_Y = 1100       # imported Design System components sit at y 0..900; ours below


def main(which):
    fid, P, DS = setup_cand()
    call("import_components", {"fileId": fid, "fromPageId": DS.id, "pageId": P.id})
    import cand_components as cc
    if "components" in which:
        names = set(cc.LIB_NAMES)
        print("deleted", delete_named(P, lambda n: n in names or n.startswith("Library / ")))
        cc.build(P, 0, LIB_Y)
        print("components", len(cc.LIB_NAMES))
    # sections go below the component library
    bottoms = [c.get("y", 0) + c["height"] for c in top_children(P) if not c["name"].startswith("Section / Candidate")]
    y0 = int(max(bottoms)) + 200
    for mod in ("access", "challenge"):   # cand_access builds Access + Session
        if mod in which or (mod == "access" and "session" in which):
            m = __import__(f"cand_{mod}")
            print(mod, m.build(P, y0))


if __name__ == "__main__":
    main(set(sys.argv[1:]) or {"components", "access", "session", "challenge"})
