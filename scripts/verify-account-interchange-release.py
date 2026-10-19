#!/usr/bin/env python3
"""Read-only live binary and authoritative static-asset identity check."""
from pathlib import Path
import hashlib
import json
import re
import subprocess
import urllib.request

root=Path(__file__).resolve().parents[1]
expected='6f58bf2c8fa9be7b622191c5e583a55fce8dbdb3b69b9c791a898ee08baa0c62'
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
assert subprocess.check_output(['systemctl','is-active','webclx.service'],text=True).strip()=='active'
pid=subprocess.check_output(['systemctl','show','-p','MainPID','--value','webclx.service'],text=True).strip()
assert digest(Path('/home/bin/webclx/webClx'))==expected
assert digest(Path('/proc')/pid/'exe')==expected
assert (root/'static').is_symlink() and str((root/'static').resolve())=='/home/bin/webclx/static'
assert not Path('/home/bin/webclx/static').is_symlink()
opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
headers={'X-WebClx-Local-Token':Path('/home/bin/webclx/.webclx-local-api-token').read_text().strip()}
checked=0
for line in (root/'docs/archive/audits/account-interchange-20261017/STATIC_SHA256SUMS').read_text().splitlines():
 sha,name=line.split(None,1)
 assert digest(root/name)==sha
 url='http://127.0.0.1:11111/assets/'+Path(name).name
 with opener.open(urllib.request.Request(url,headers=headers),timeout=20) as response:
  assert hashlib.sha256(response.read()).hexdigest()==sha,'HTTP static identity mismatch: '+name
 checked+=1
with opener.open(urllib.request.Request('http://127.0.0.1:11111/',headers=headers),timeout=20) as response:
    assert response.geturl()=='http://127.0.0.1:11111/login','unauthenticated shell must redirect to login'
    shell=response.read().decode()
# The login page adds the hostname inside <title>; all other bytes must match.
strip_title=lambda value:re.sub(r'<title>.*?</title>','<title></title>',value,count=1,flags=re.S)
assert strip_title(shell)==strip_title((root/'static/login.html').read_text()),'served login page differs beyond dynamic hostname title'
print(json.dumps({'service':'webclx.service','pid':pid,'running_binary_sha256':expected,'http_static_verified':checked,'account_writes':0}))
