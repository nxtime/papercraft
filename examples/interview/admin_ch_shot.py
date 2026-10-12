"""python3 admin_ch_shot.py <Section name> [screen-substring] -> PNGs in scratchpad."""
import sys
sys.path.insert(0, "/Volumes/Storage/Projects/papercraft/examples/interview")
from admin_ch_common import *
OUT = "/private/tmp/claude-501/-Volumes-Storage-Projects-photocraft/11f4040a-b4f4-4b9e-965a-bde08b400450/scratchpad/"
fid, P = init()
sec = next(c for c in P.c("get_children", nodeId="root")["children"] if c["name"] == sys.argv[1])
def walk(n, d=0):
    for c in P.c("get_children", nodeId=n)["children"]:
        yield c
        if d < 3 and c["name"].startswith(("Screens","Row ","Frame /")): yield from walk(c["id"], d+1)
want = sys.argv[2] if len(sys.argv) > 2 else ""
for c in walk(sec["id"]):
    if c["name"].startswith("Screen / ") and want in c["name"]:
        print(c["name"], c["width"], c["height"])
        P.screenshot(OUT + c["name"].replace(" / ", "_").replace(" ", "_") + ".png", c["id"], 1)
