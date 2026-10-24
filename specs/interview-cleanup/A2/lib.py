import json,subprocess
def call(tool,**a):
    r=subprocess.run(['papercraft-cli','call',tool,'-'],input=json.dumps(a),capture_output=True,text=True)
    try: return json.loads(r.stdout)
    except Exception: return {'_raw':r.stdout,'_err':r.stderr}
