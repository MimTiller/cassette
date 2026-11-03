import json, urllib.request
tabs = json.load(urllib.request.urlopen("http://127.0.0.1:9341/json/list", timeout=5))
for t in tabs:
    print(t.get("url"), t.get("id"))
