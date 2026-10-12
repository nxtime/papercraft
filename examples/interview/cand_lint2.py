from cand_common import *
import collections
fid,P,_=setup_cand()
def walk(n,out):
    for c in P.c("get_children",nodeId=n)["children"]:
        if c["name"].startswith("Screen /"): out.append(c)
        elif c["name"].startswith(("Screens","Block","Section / Candidate")): walk(c["id"],out)
out=[]
for c in top_children(P):
    if c["name"] in ("Section / Candidate Access","Section / Candidate Session"): walk(c["id"],out)
for c in out:
    r=P.lint(c["id"]); iss=r["issues"]
    print(c["name"],r.get("errors"),r.get("warnings"),r.get("byRule"))
    seen=set()
    for i in iss:
        k=(i.get("severity"),str(i.get("message"))[:70])
        if i.get("severity") in ("error","warning") and k not in seen: seen.add(k); print("   ",k)
