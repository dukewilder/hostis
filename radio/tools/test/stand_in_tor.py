#!/usr/bin/env python3
"""A stand-in for Tor's HTTP tunnel, for tests: answers CONNECT host:port by opening the
connection and piping both ways, and writes each target it was asked for to a file, one a line.
Also answers plain GET http://... by fetching it, so a fetch through the tunnel is seen too.
Usage: stand_in_tor.py <port> <file of targets>. No rights reserved."""
import socket, sys, threading

PORT, OUT = int(sys.argv[1]), sys.argv[2]
lock = threading.Lock()

def note(t):
    with lock, open(OUT, "a") as f:
        f.write(t + "\n")

def pipe(a, b):
    try:
        while True:
            d = a.recv(65536)
            if not d:
                break
            b.sendall(d)
    except OSError:
        pass
    for s in (a, b):
        try: s.shutdown(socket.SHUT_RDWR)
        except OSError: pass

def serve(c):
    head = b""
    while b"\r\n\r\n" not in head:
        d = c.recv(4096)
        if not d:
            c.close(); return
        head += d
    line = head.split(b"\r\n", 1)[0].decode("latin-1")
    method, target = line.split(" ")[:2]
    if method == "CONNECT":
        host, port = target.rsplit(":", 1)
        note("CONNECT " + target)
        try:
            u = socket.create_connection((host.strip("[]"), int(port)), timeout=10)
        except OSError:
            c.sendall(b"HTTP/1.1 502 Bad Gateway\r\n\r\n"); c.close(); return
        u.settimeout(None)
        c.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
        rest = head.split(b"\r\n\r\n", 1)[1]
        if rest:
            u.sendall(rest)
        threading.Thread(target=pipe, args=(c, u), daemon=True).start()
        pipe(u, c)
    else:
        note(method + " " + target)
        c.sendall(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        c.close()

s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", PORT)); s.listen(64)
while True:
    c, _ = s.accept()
    threading.Thread(target=serve, args=(c,), daemon=True).start()
