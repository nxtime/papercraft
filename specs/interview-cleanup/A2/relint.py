from lib import *
import json,collections,sys
ab=call('get_basic_info',pageId='64e1d4aaf56fe6a4')['artboards']
allis=[]
for a in ab:
    r=call('lint_design',pageId='64e1d4aaf56fe6a4',nodeId=a['id'])
    for i in r.get('issues',[]): i['top']=a['id'];i['topname']=a['name'];allis.append(i)
json.dump(allis,open(sys.argv[1],'w'))
print(len(ab),len(allis),collections.Counter(i['rule'] for i in allis))
