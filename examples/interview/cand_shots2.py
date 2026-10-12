import sys
from cand_common import *
fid,P,_=setup_cand()
want=sys.argv[1:]
def walk(n,out):
    for c in P.c("get_children",nodeId=n)["children"]:
        out.append(c); 
        if c["name"].startswith(("Screens","Block","Section")) : walk(c["id"],out)
out=[];walk("root",out)
for c in out:
    if c["name"].startswith("Screen /") and (not want or any(w in c["name"] for w in want)):
        print(shot(P,c["id"],c["name"].replace(" ","_").replace("/","")))
