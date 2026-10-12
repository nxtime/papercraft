import sys; sys.path.insert(0,"/Volumes/Storage/Projects/papercraft/examples/interview")
from ds_common import *
fid,P=setup()
out="/private/tmp/claude-501/-Volumes-Storage-Projects-photocraft/11f4040a-b4f4-4b9e-965a-bde08b400450/scratchpad/"
want=sys.argv[1:]
for c in P.c("get_children",nodeId="root")["children"]:
    if c["name"] in want or (want==["all"]):
        print(c["name"],c["width"],c["height"]); P.screenshot(out+c["name"].replace(" ","_").replace("/","")+".png",c["id"],1)
