#!/usr/bin/env python3
"""players.txt moves the open player's fixed name only when two release keys signed it.
Throwaway keys sign a list with `release sign-all`; verify_players.py and pin_player.sh (with a
stand-in ipfs) take it with two signatures, keep the last name with one, and refuse a shorter
list. Usage: players_sigs.py <release binary>. Prints PASS or FAIL lines."""
import os, subprocess, sys, tempfile

R = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
REL = sys.argv[1]
sys.path.insert(0, f"{R}/tools")
import verify_players as vp

ok = True
def check(name, cond):
    global ok
    print(("PASS " if cond else "FAIL ") + name)
    ok &= bool(cond)

check("the verifier carries the app's release keys", vp.KEYS == open(f"{R}/app/assets/release_keys.txt").read().split())

t = tempfile.mkdtemp()
subprocess.run([REL, "init", f"{t}/keys"], check=True, capture_output=True)
pubs = open(f"{t}/keys/release_keys.txt").read().split()
env = dict(os.environ, PYRATE_RELEASE_KEYS=",".join(pubs))
site = f"{t}/site"; os.makedirs(site)
open(f"{site}/players.txt", "w").write("bafyold\nbafynew\n")

def sign(n):
    r = subprocess.run([REL, "sign-all", f"{t}/keys/release-key-{n}.txt", str(n), site], env=env, capture_output=True, text=True)
    return r.stdout

out = sign(1)
check("one holder signs players.txt", "Signed players.txt." in out and "One more holder" in out)
verify = lambda: subprocess.run([sys.executable, f"{R}/tools/verify_players.py", "--keys", f"{t}/keys/release_keys.txt", f"{site}/players.txt", f"{site}/players.sigs"], capture_output=True, text=True)
check("one signature is refused", verify().returncode == 1)
wrong = subprocess.run([REL, "sign-all", f"{t}/keys/release-key-2.txt", "3", site], env=env, capture_output=True, text=True)
check("words of another key number are refused", wrong.returncode != 0 and "not release key 3" in wrong.stderr)
out = sign(3)
r = verify()
check("two signatures pass and name the newest", r.returncode == 0 and r.stdout.split() == ["2", "bafynew"])
chk = subprocess.run([REL, "check", site], env=env, capture_output=True, text=True)
check("check says the folder is signed", chk.returncode == 0 and "players.txt is signed by two keys. Ready." in chk.stdout)

# A release signature over the same bytes never passes as a players one.
import struct, hashlib
check("a players signature is not a release signature", vp.message(b"x") != b"pyrate/release/v1\0" + struct.pack("<I", 0) + hashlib.sha256(b"x").digest())

# pin_player.sh against a stand-in ipfs that pins everything and records each name.
bin_ = f"{t}/bin"; os.makedirs(bin_)
log = f"{t}/ipfs.log"
open(f"{bin_}/ipfs", "w").write(f"#!/bin/sh\necho \"$@\" >> {log}\nexit 0\n")
os.chmod(f"{bin_}/ipfs", 0o755)
tools = f"{t}/tools"; os.makedirs(tools)
for f in ("pin_player.sh", "verify_players.py"):
    s = open(f"{R}/tools/{f}").read()
    if f == "verify_players.py":
        s = s.replace("KEYS = [", "KEYS = " + repr(pubs) + "\n_OLD = [")
    open(f"{tools}/{f}", "w").write(s)
penv = dict(os.environ, PATH=f"{bin_}:{os.environ['PATH']}", BASE=f"file://{site}", IPNS_KEY="pyrate-player", STATE=f"{t}/state")
def pin():
    open(log, "w").close()
    r = subprocess.run(["sh", f"{tools}/pin_player.sh"], env=penv, capture_output=True, text=True)
    names = [l for l in open(log) if l.startswith("name publish")]
    return r, names
r, names = pin()
check("pin_player.sh names the signed newest", r.returncode == 0 and len(names) == 1 and "/ipfs/bafynew" in names[0])

open(f"{site}/players.txt", "w").write("bafyold\nbafynew\nbafyevil\n")
r, names = pin()
check("an unsigned list leaves the name on the last signed player", len(names) == 1 and "/ipfs/bafynew" in names[0] and "not signed" in r.stdout)

open(f"{site}/players.txt", "w").write("bafyold\n")
sign(1); sign(2)
r, names = pin()
check("a shorter signed list is refused", len(names) == 1 and "/ipfs/bafynew" in names[0] and "fewer players" in r.stdout)

penv["SIGNED_CID"] = "2 bafyevil"
open(f"{site}/players.txt", "w").write("bafyold\nbafynew\n")
r, names = pin()
check("a host's answer naming a player off the list is refused", "/ipfs/bafynew" in names[0] and "not in players.txt" in r.stdout)

sys.exit(0 if ok else 1)
