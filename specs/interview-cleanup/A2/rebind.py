from lib import *
import json,re,collections,sys
P='64e1d4aaf56fe6a4'
flat=json.load(open('flat.json'));rev={n.replace('/','-'):n for n in flat}
I=json.load(open(sys.argv[1]))
hc=[i for i in I if i['rule']=='hard-coded-color' and 'is the value' in i['message']]
ids=sorted({i['node'] for i in hc});st={}
for k in range(0,len(ids),40): st.update(call('get_computed_styles',pageId=P,nodeIds=ids[k:k+40]))
g=collections.defaultdict(list);f=collections.defaultdict(list)
for i in hc:
    hx,css=re.match(r'(#\w+) is the value of --(\S+)',i['message']).groups()
    s=st[i['node']]
    if s.get('backgroundColor','').lower()==hx and s.get('color','').lower()!=hx: f[css].append(i['node']);continue
    if hx=='#ffffff': css='color-text-inverse'
    if hx in('#0958d9','#1677ff'): css='color-text-link'
    g[css].append(i['node'])
ups=[{'nodeIds':n,'styles':{'color':f'var(--{c})'}} for c,n in g.items()]+[{'nodeIds':n,'styles':{'backgroundColor':f'var(--{c})'}} for c,n in f.items()]
print(call('update_styles',pageId=P,updates=ups))
