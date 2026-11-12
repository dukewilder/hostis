"""Writes the machine-readable test manifest for a release from the evidence files.

    python3 tools/audit/manifest.py <version> [artifact files...]

Each requirement names the line of evidence that proves it. A requirement passes only if every
line it names is in its evidence file and begins with PASS; a line beginning with FAIL, or no
line at all, fails it. Artifacts are hashed as given."""
import hashlib, json, os, platform, subprocess, sys

R = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
EV = "tools/audit/evidence"
REG = f"{EV}/regression-0.9.13.txt"
F4 = f"{EV}/browser-f4-0.9.13.txt"
TESTS = f"{EV}/tests-0.9.13.txt"
PRIV = f"{EV}/privacy-0.9.13.txt"
H = "PYRATE_NULL_AUDIO=1 PYRATE_LOG_MB=64 PYRATE_LAN_ANY=1 target/release/pentest <dir> <wav>"

# id, gate, finding, what must hold, runner, evidence, line prefixes, baseline, fix commits
REQ = [
    ("NET-001", 1, "F1", "a station's real carrier is found despite 64 false claims that came first", f"{H} eclipse", REG, ["PASS carrier eclipse"], "FAIL on db87162: never found in 240 s", "8d78947 0368370"),
    ("ARC-001", 1, "F2", "carried logs stay within their budget; cardless stations refused; memory comes back", f"{H} logflood", REG, ["PASS log flood: 20 stations", "PASS log flood: logs on disk", "PASS log flood: target 3", "PASS log flood: target memory", "PASS log flood: a listener", "PASS log flood: the station carried"], "398 MB of logs in 60 s, no bound", "7973146 30714ef"),
    ("ARC-003", 1, "F6", "log replies are paged by bytes and their memory shared and fixed", f"{H} logreply", REG, ["PASS log replies: 64 answered", "PASS log replies: target", "PASS log replies: a listener", "PASS log replies: the whole log"], "target killed at 6,040 MB", "7973146 8d78947"),
    ("KEY-009", 1, "F3", "signed bytes after a card's fields are bounded on the wire", f"{H} cardwire", REG, ["PASS card with 512", "PASS card with 513", "PASS card with 120000"], "120 KB tail accepted", "7973146"),
    ("KEY-001", 1, "F5", "a revoked deputy's card shows no tip, no cover and none of its words as the station's", f"{H} deputy", REG, ["PASS revoked deputy, fresh listener: no tip", "PASS revoked deputy, fresh listener: no cover", "PASS revoked deputy, fresh listener: none of its words"], "name 'Hijacked' shown", "0368370 44c4036"),
    ("DOS-002", 1, "F4", "a browser tab under slow, flood and oversized requests keeps playing within memory", "tools/test/browser_attack.py <port> <relay> pentest <slow|flood|big> 60", F4, ["PASS slow: the tab kept playing", "PASS slow: browser memory", "PASS flood: the tab kept playing", "PASS flood: browser memory", "PASS big: the tab kept playing", "PASS big: browser memory"], "see browser-f4-0.9.12.txt", "7973146"),
    ("DOS-007", 1, "F7", "every web route answers at p99 under 1 s under 64 clients, a listener served", f"{H} now routes", REG, ["PASS web /now/", "PASS web /feed/", "PASS web /log.json", "PASS web /:", "PASS web /art/", "PASS web /episode/", "PASS web /nothing"], "/now p99 9,966 ms", "7973146"),
    ("KEY-008", 1, "N1", "an entry nested 40,000 deep does not stop the node", f"{H} nest", REG, ["PASS nested entry 40,000 deep"], "stack overflow at 30,000 deep", "9468541"),
    ("DOS-008", 1, "N5", "the costliest schedule a station can sign costs its listeners little", f"{H} schedule", REG, ["PASS costly schedule: the day's bill", "PASS costly schedule: what is on", "PASS costly schedule: tuned and playing", "PASS costly schedule: a listener"], "bill 6,537 ms with the store locked, 14% of a core playing (3885b65)", "bd3951e 528a0e0"),
    ("DOS-001", 2, "0.9.12", "stem floods, slow readers, stream floods and junk are bounded", f"{H} stems slow flood", REG, ["PASS stem flood: 1", "PASS stem flood: a listener", "PASS junk stem", "PASS slow readers: 640", "PASS slow readers: still", "PASS 400 identities"], "2.6 GB in 25 s (0.9.11)", "0.9.12"),
    ("FUZ-001", 2, "0.9.12", "mutated frames and requests panic nothing", f"{H} fuzz netfuzz", REG, ["PASS decoder fuzz", "PASS network fuzz"], "", "0.9.12"),
    ("NET-002", 2, "0.9.12", "a declined stem is handled once between two Lanterns", f"{H} loop", REG, ["PASS declined stem"], "", "0.9.12"),
    ("WEB-001", 2, "0.9.12", "half-open web requests hold nothing", f"{H} web", REG, ["PASS web: 2", "PASS web: still served"], "", "0.9.12"),
    ("KEY-005", 2, "0.9.12", "an oversized card never reaches the Log", f"{H} cards", REG, ["PASS oversized card"], "200,000 characters shown (0.9.11)", "0.9.12"),
    ("KEY-002", 2, "", "each kind of entry takes only the keys allowed it; no move, replay, fork or backdate", "cargo test -p pyrate-core --test authority", TESTS, ["test each_kind_of_entry_takes_only_the_keys_allowed_it ... ok", "test a_signed_entry_cannot_be_moved_replayed_forked_or_backdated ... ok"], "", "d31f186"),
    ("FUZ-002", 2, "N4 N5", "random signed logs never make the clock panic or take a second; the quick clock agrees with the walking one", "cargo test -p pyrate-core --test clock_fuzz", TESTS, ["test the_clock_survives_any_log ... ok", "test passing_over_changes_nothing_on_any_log ... ok", "test passing_over_changes_nothing_on_any_rota ... ok", "test remembered_answers_are_the_walked_answers ... ok", "test the_costliest_schedules_a_station_can_sign ... ok", "test a_rota_block_naming_thousands_of_recordings ... ok"], "4.7 s to say what is on, 82 s for a bill", "bd3951e"),
    ("ARC-004", 2, "N6", "a log torn by a crash is cut back and keeps what comes after", "cargo test -p pyrate-node --lib", TESTS, ["test store::tests::a_log_torn_by_a_crash_is_cut_back_and_keeps_what_comes_after ... ok"], "", "3885b65"),
    ("ARC-007", 2, "N8", "over quota the hold lets go of orphans, then the largest carried station, never our own", "cargo test -p pyrate-node --lib", TESTS, ["test store::tests::over_quota_the_hold_lets_go_of_orphans_then_the_largest_carried_station_never_our_own ... ok"], "769 MB spike, listener 7.3 s (528a0e0)", "30714ef"),
    ("PRV-001", 0, "Gate 0", "privacy checks, the two that need playback included", "PYRATE_NULL_AUDIO=1 PYRATE_LAN_ANY=1 target/release/privacy_test <dir> <wav>", PRIV, ["PASS a phone asks carriers directly only when its owner says so", "PASS with one node known and no two-hop route, nothing plays", "PASS and the carrier was asked for nothing directly", "PASS a broadcasting phone with one node in reach", "PASS with the switch on, it plays", "PASS with a Lantern beside the keeper", "PASS with a second node in reach"], "two playback checks failed with no speaker", "3885b65"),
]


def sha(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def result(evidence, lines):
    p = os.path.join(R, evidence)
    if not os.path.exists(p):
        return "NOT RUN", []
    text = open(p, encoding="utf-8", errors="replace").read().splitlines()
    missing = [l for l in lines if not any(t.startswith(l) for t in text)]
    failed = [t for t in text if t.startswith("FAIL") and any(t[5:].startswith(l[5:]) for l in lines if l.startswith("PASS "))]
    if failed:
        return "FAIL", failed
    return ("PASS" if not missing else "FAIL"), missing


def main():
    version = sys.argv[1]
    commit = subprocess.run(["git", "-C", R, "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    out = {
        "release": version,
        "commit": commit,
        "machine": {"os": f"{platform.system()} {platform.release()}", "cpus": os.cpu_count(), "rustc": subprocess.run(["rustc", "--version"], capture_output=True, text=True).stdout.strip()},
        "artifacts": [{"file": os.path.basename(a), "sha256": sha(a), "bytes": os.path.getsize(a)} for a in sys.argv[2:] if os.path.exists(a)],
        "requirements": [],
    }
    for rid, gate, finding, holds, runner, ev, lines, baseline, fix in REQ:
        res, notes = result(ev, lines)
        out["requirements"].append({"id": rid, "gate": gate, "finding": finding, "assertion": holds, "runner": runner, "evidence": ev, "lines": lines, "baseline": baseline, "fix": fix, "result": res, "unmet": notes})
    path = os.path.join(R, f"tools/audit/manifest-{version}.json")
    json.dump(out, open(path, "w"), indent=1)
    print(path, sum(r["result"] == "PASS" for r in out["requirements"]), "of", len(out["requirements"]), "pass")
    for r in out["requirements"]:
        if r["result"] != "PASS":
            print(" ", r["id"], r["result"], r["unmet"][:2])


main()
