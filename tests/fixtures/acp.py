"""Synthetic stdio peer. Never imports an SDK, accesses a network, or runs a model."""
import json
import os
from pathlib import Path
import sys


def emit(**fields):
    print(json.dumps({"jsonrpc": "2.0", **fields}), flush=True)


def receive(method):
    msg = json.loads(input())
    assert msg["jsonrpc"] == "2.0" and msg["method"] == method
    return msg


def update(kind, **fields):
    emit(method="session/update", params={
        "sessionId": "fixture", "update": {"sessionUpdate": kind, **fields}
    })


mode, literal = sys.argv[1:]
assert literal == "argument with spaces"
init = receive("initialize")
assert init["params"]["protocolVersion"] == 2
emit(id=init["id"], result={
    "protocolVersion": 1 if mode == "v1" else 2,
    "info": {"name": "fixture", "version": "1"},
    "capabilities": {} if mode == "no-session" else {"session": {"additionalDirectories": {}}}
})
new = receive("session/new")
assert Path(new["params"]["cwd"]) == Path.cwd() == Path(os.environ["FERRISCOPE_PR_BUNDLE"])
manifest = json.loads(Path("manifest.json").read_text())
assert len(manifest) == 2 and Path(manifest[0]["patch"]).is_file()
source = Path(os.environ["FERRISCOPE_PR_SOURCE"])
assert new["params"]["additionalDirectories"] == [str(source)]
assert (source / "unchanged").read_text() == "fixture"
emit(id=new["id"], result={"sessionId": "fixture"})
prompt = receive("session/prompt")
assert prompt["params"]["sessionId"] == "fixture"
emit(id=prompt["id"], result={"messageId": "user"})
update("user_message", messageId="user", content=prompt["params"]["prompt"])
update("state_update", state="running")
if mode == "cancel":
    assert receive("session/cancel")["params"]["sessionId"] == "fixture"
    (source / "cancelled").touch()
    update("state_update", state="idle", stopReason="cancelled")
    sys.exit()
if mode == "malformed":
    print("not json", flush=True)
    sys.exit()
emit(id="perm", method="session/request_permission", params={
    "sessionId": "fixture", "title": "Allow fixture operation?",
    "options": [{"kind": "allow_once", "optionId": "yes", "name": "Allow"}]
})
reply = json.loads(input())
assert reply["id"] == "perm" and reply["result"]["outcome"] == {"outcome": "cancelled"}
route = json.dumps({"pr": "demo/service/pull/1", "head": "rev", "stops": [
    {"path": f["path"], "line": 1, "side": "RIGHT", "why": "fixture"} for f in manifest
]})
if mode == "invalid":
    route = "not a walkthrough"
def text(value):
    return {"type": "text", "text": value}


update("agent_message", messageId="progress", content=[text("Reading the patches")])
update("agent_message_chunk", messageId="answer", content=text("superseded"))
update("agent_message", messageId="answer", content=None)
for chunk in [route[:10], route[10:]]:
    update("agent_message_chunk", messageId="answer", content=text(chunk))
update("agent_message", messageId="progress", content=[text("Read both patches")])
update("tool_call_update", toolCallId="tool", rawOutput="never the answer")
if mode == "crash":
    os._exit(17)  # A complete-looking answer still isn't a completed turn.
update("state_update", state="idle", stopReason="end_turn")
