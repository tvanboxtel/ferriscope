#!/usr/bin/env python3
"""Deterministic stdio peer. The test installs this outside the checkout."""
import json
from pathlib import Path
import sys
from urllib.parse import unquote, urlsplit

def read():
    headers = {}
    for line in sys.stdin.buffer:
        if line == b"\r\n":
            body = sys.stdin.buffer.read(int(headers[b"content-length"]))
            return json.loads(body)
        name, value = line.split(b":", 1)
        headers[name.lower()] = value.strip()

def reply(request, result):
    data = json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}).encode()
    sys.stdout.buffer.write(f"Content-Length: {len(data)}\r\n\r\n".encode() + data)
    sys.stdout.buffer.flush()

docs = {}
while msg := read():
    method = msg["method"]
    if method == "initialize":
        root = Path(unquote(urlsplit(msg["params"]["rootUri"]).path))
        mode = Path(__file__).with_name("mode").read_text()
        encoding = mode if mode.startswith("utf-") else "utf-16"
        reply(msg, {"capabilities": {"positionEncoding": encoding, "definitionProvider": True}})
    elif method == "textDocument/didOpen":
        doc = msg["params"]["textDocument"]
        path = Path(unquote(urlsplit(doc["uri"]).path))
        assert path.parent == root and doc["text"] == path.read_text()
        docs[doc["uri"]] = doc
    elif method == "textDocument/definition":
        doc = docs[msg["params"]["textDocument"]["uri"]]
        nested = doc["uri"] == (root / "library.ts").as_uri()
        name = "answer" if nested else "alias"
        line, call = next((i, text) for i, text in enumerate(doc["text"].splitlines()) if name+"()" in text)
        prefix = call.split(name)[0]
        column = {"utf-8": len(prefix.encode()), "utf-16": len(prefix.encode("utf-16-le")) // 2,
                  "utf-32": len(prefix)}[encoding]
        assert msg["params"]["position"] == {"line": line, "character": column}
        pos = {"start": {"line": 1, "character": 16}, "end": {"line": 1, "character": 30}}
        if nested:
            pos = {"start":{"line":4,"character":9},"end":{"line":4,"character":15}}
        uri = (Path(__file__) if mode == "outside" else root / "library.ts").as_uri()
        reply(msg, None if mode == "none" else [{"targetUri": uri, "targetRange": pos, "targetSelectionRange": pos}])
    elif method == "shutdown":
        reply(msg, None)
    elif method == "exit":
        break
