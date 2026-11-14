#!/usr/bin/env python3
# The launch pass (November 2026), run after build_pass.py: what tools/stations/*.json gain for
# launch, and three new stations. Every licence is read from its archive.org item and checked as
# build_pass.py checks it; anything not free to air is left out and named.
#   build_launch.py
# Writes beside this script:
#   netlabel.json, poets.json, contraband.json   (each gains the items below)
#   shanties.json                                (sea songs and shanties alone; its bands go to Brass)
#   readings.json, public-domain.json            (what those two stations gain, fetched into their folders)
#   drift.json, concert-hall.json, brass.json    (new stations)
# No rights reserved.
import json, os, re, sys
import build_pass as bp

HERE = bp.HERE

# Netlabel gains upbeat electronic, IDM, minimal techno and dubstep, each release whole.
NETLABEL = ["MNF033_AFM--Sometimes_EP", "MNF027_Dsum--Intimate", "MNF025_DCM--Tunnel_EP", "MNF023_Dsum_Morris--Arium_FL",
            "Monokrak209FloatingMind_Ready_For_Flying", "Monokrak211FloatingMind_BroussaillesEP", "Monokrak213FloatingMind_Cleave",
            "Monokrak215FloatingMind_Morbida", "USR-002", "USR-007", "USR-012", "USR-017", "USR-021", "mt006", "mt012", "acediamusic038",
            "shoki005g", "shoki006", "prak002", "unfound91", "enrmp262_rolemusic_-_straw_fields", "tranz060TaleTwist-WryWay",
            "tranzcd009Va-TranzmitterResearchvolume3", "TAM026-microfluid", "rfl009", "Robot_After_All_Volume_1", "yarn013", "yarn014",
            "yarn020", "yarn022", "gt459AnchoreState-ChangesOfLifeEp", "gt500GeneticTrance-Remixes", "gt390FandorinProjet2"]
# Drift: long ambient, drone and piano for work and sleep.
DRIFT = ["bof056", "bof060", "bof064", "rz145", "rz106", "gt458SteamFlow-WithTrainsAndBones", "gt577Noisesurfer-RainTearDrops",
         "gt536Maelstrom-ThePassage", "Escape_From_Lhasa", "MNF041_Callejas--Paramo", "MNF036_Insanity_Circle--Itinerario_Nocturno",
         "PILOTELEVEN_017", "PILOTELEVEN_018", "unfound88", "koraii-full-discography", "ChrisZabriskieDirectToVideo",
         "ChrisZabriskieReappear", "ChrisZabriskieIAmAManWhoWillFightForYourHonor", "ChrisZabriskieThoughtless"]
# Concert Hall: works in the public domain, played by performers who freed the recordings, or by
# a federal band.
CONCERT = ["The_Open_Goldberg_Variations-11823", "bach-well-tempered-clavier-book-1",
           "pandacd-715-js-bach-the-art-of-the-fugue-kunst-der-fuge-bwv-1080", "musopen-dvd-in-lossless-flac-format", "musopen-chopin",
           "Classical_Sampler-9615", "post009", "TheFourSeasonsWinter", "StraussWaltz", "AirSuiteNo.3InDMajor"]
# Brass: every Sousa march the Marine Band recorded to 1926, other federal bands' public domain
# pieces, and the band records that leave Shanties.
BRASS = [f"InternetJukebox.JPS.{n}" for n in range(10, 107)] + [
    "ServiceSongs", "NavyHymn", "AmericaTheBeautiful", "FlightOfTheBumblebee", "TheBattleHymnOfTheRepublic_993",
    "AmericaTheBeautiful_613", "LibertyBellMarch_747", "TurkeyInTheStraw", "LochLomond_32", "MarinesHymn"]
# The Public Domain gains 1920s tunes in newer recordings by the U.S. Air Force Band.
PUBLIC_DOMAIN = ["FarewellBlues", "TheWabashBlues", "MilenbergJoys", "GeorgeM.CohanMedley"]
# Federal bands: works of the U.S. Government (17 U.S.C. 105).
FEDERAL = re.compile(r"InternetJukebox|ServiceSongs|NavyHymn|AmericaTheBeautiful|FlightOfTheBumblebee|BattleHymn|LibertyBell|TurkeyInTheStraw|"
                     r"LochLomond|MarinesHymn|FarewellBlues|WabashBlues|MilenbergJoys|CohanMedley|FourSeasons|StraussWaltz|AirSuite")

# Books, read by LibriVox volunteers, in the order they air: (item, author, title, sections).
READINGS = [
    ("captain_blood_librivox", "Rafael Sabatini", "Captain Blood", None),
    ("sea_hawk_0710_librivox", "Rafael Sabatini", "The Sea Hawk", None),
    ("sea_wolf_0907_librivox", "Jack London", "The Sea-Wolf", None),
    ("ghost_pirates_1305_librivox", "William Hope Hodgson", "The Ghost Pirates", None),
    ("narrative_gordon_pym_librivox", "Edgar Allan Poe", "The Narrative of Arthur Gordon Pym", None),
    ("moby_dick_librivox", "Herman Melville", "Moby-Dick", None),
    ("typee_librivox", "Herman Melville", "Typee", None),
    ("twoyearsmast_1306_librivox", "Richard Henry Dana Jr.", "Two Years Before the Mast", None),
    ("kidnapped_0807_librivox", "Robert Louis Stevenson", "Kidnapped", None),
    ("robinson_crusoe_anew_for_children_librivox", "James Baldwin", "Robinson Crusoe Written Anew for Children", None),
    ("lord_jim_librivox", "Joseph Conrad", "Lord Jim", None),
    ("heart_of_darkness", "Joseph Conrad", "Heart of Darkness", None),
]
CONTRABAND = [
    ("ulysses_2007_librivox", "James Joyce", "Ulysses", None),
    ("uncle_toms_cabin_librivox", "Harriet Beecher Stowe", "Uncle Tom's Cabin", None),
    ("leaves_of_grass_librivox", "Walt Whitman", "Leaves of Grass", None),
    ("dorian_gray_librivox", "Oscar Wilde", "The Picture of Dorian Gray", None),
    ("rightsofman_1502_librivox", "Thomas Paine", "Rights of Man", None),
    ("age_reason_0910_librivox", "Thomas Paine", "The Age of Reason", None),
    ("communist_manifesto_mfs_librivox", "Karl Marx and Friedrich Engels", "The Communist Manifesto", None),
    ("conquest_of_bread_0809_librivox", "Peter Kropotkin", "The Conquest of Bread", None),
    ("mutual_aid_0808_librivox", "Peter Kropotkin", "Mutual Aid", None),
    ("ten_days_1203_librivox", "John Reed", "Ten Days That Shook the World", None),
    ("origin_species_librivox", "Charles Darwin", "On the Origin of Species", None),
    ("portrait_artist_pb_librivox", "James Joyce", "A Portrait of the Artist as a Young Man", None),
    ("dubliners_0906_librivox", "James Joyce", "Dubliners", None),
    # Both first published in 1928, in the US public domain since 1 January 2024.
    ("ladychatterleyslover_2404_librivox", "D. H. Lawrence", "Lady Chatterley's Lover", None),
    ("wellofloneliness_2506_librivox", "Radclyffe Hall", "The Well of Loneliness", None),
]
# An in-order station starts again from the top each Monday, so it holds a week at most (168 h).
# These wait for a rotation: in Readings the children's Crusoe; in Contraband the four books
# with the thinnest case for having been banned.
HELD = {"robinson_crusoe_anew_for_children_librivox", "origin_species_librivox", "ten_days_1203_librivox",
        "mutual_aid_0808_librivox", "conquest_of_bread_0809_librivox"}
# Poets' Corner gains sea verse, ballads and Blake. One reading of each poem; a poem the station
# already has keeps the reading it has.
# Left out: Poems of the Great War, whose recording names no poet for any poem.
POEMS = ["sea_fever_librivox", "rime_ancient_mariner_librivox", "songs_of_travel_and_other_verses_2309_librivox",
         "barrack-room_ballads_2201_librivox", "poems_wm_blake_ss_librivox", "songs_innocence_experience_librivox"]

# Shanties keeps shanties and sea songs alone. Band records leave for Brass; marches that are not
# band records, and field recordings the Library of Congress does not file as sea songs, retire.
TO_BRASS = {"loc-jukebox-654579-row-well-ye-mariners", "78_the-pirates-of-penzance-part-i_the-band-of-hm-scots-guards-sullivan_gbia3015847a",
            "78_the-pirates-of-penzance-part-ii_the-band-of-hm-scots-guards-sullivan_gbia3015847b", "loc-jukebox-128850-flying-dutchman-overture",
            "78_jack-tar-march_sousas-band_gbia0011119a", "loc-jukebox-124836-ship-ahoy-march", "loc-jukebox-248883-blue-jackets-march",
            "edison-50995_01_5617", "loc-jukebox-42208-anchors-aweigh", "RoastBeefOfEngland"}
RETIRE = {"78_the-anchors-weighed-march_peter-and-daniel-wyper_gbia3036507a", "78_death-of-nelson-march_peter-and-daniel-wyper_gbia3036507b",
          "afc1940001_a3816b1", "afc1940001_a4206b1", "afc1940001_a3354b1", "afc1940001_a3358a1"}

# Compilations, whose files are named "07 Artist - Title".
COMPILATIONS = {"gt390FandorinProjet2", "gt500GeneticTrance-Remixes", "USR-021", "Robot_After_All_Volume_1",
                "tranzcd009Va-TranzmitterResearchvolume3", "shoki005g"}
TITLE = {"InternetJukebox.JPS.98": "Nobles of the Mystic Shrine"}
# Names as the makers write them, where a tag got them wrong.
BY = {"FloatingMind": "Floating Mind", "Unregistered version": "Tim Doyle", "www.musopen.org": "Musopen", "mikael lind": "Mikael Lind",
      "rolemusic": "Rolemusic", '"The President\'s Own" United States Marine Band': "United States Marine Band"}

skipped = []


def unmangle(s):
    """Cyrillic tags read as Latin-1 (Ñòàðèíà) back into Cyrillic."""
    if re.search(r"[\u00c0-\u00ff]{2}", s):
        try:
            t = s.encode("latin-1").decode("cp1251")
            if re.search(r"[\u0400-\u04ff]", t):
                return t
        except UnicodeError:
            pass
    return s


def free(md, item):
    try:
        return bp.licence(md, item)
    except SystemExit as e:
        skipped.append(str(e))
        return None


def first(v):
    return (v[0] if v else "") if isinstance(v, list) else (v or "")


def music(items, lo=30, hi=4 * 3600):
    out = []
    for item in items:
        m = bp.meta(item)
        md = m["metadata"]
        got = free(md, item)
        if not got:
            continue
        lic, name = got
        if FEDERAL.search(item):
            name = "Public domain in the United States, a work of the U.S. Government (17 U.S.C. 105)"
        album = bp.clean(first(md.get("title")))
        files = [f for f in m["files"] if f.get("source") == "original" and f["name"].lower().endswith(".mp3")]
        if not files:
            files = [f for f in m["files"] if f.get("format") == "VBR MP3"]
        if not files:
            # Released as lossless alone: fetch.py keeps the file's own extension.
            files = [f for f in m["files"] if f.get("format") == "Flac"]
        # Some labels uploaded smaller copies beside each track and marked them original too.
        names = {f["name"] for f in files}
        files = [f for f in files if not (re.search(r"_(64kb|vbr)\.mp3$", f["name"]) and re.sub(r"_(64kb|vbr)\.mp3$", ".mp3", f["name"]) in names)]
        lead = album.split(" - ")[0].strip() if " - " in album else ""
        twice = set()
        for f in sorted(files, key=lambda f: f["name"]):
            if not lo <= bp.seconds(f.get("length")) <= hi:
                continue
            by = bp.clean(first(f.get("artist")) or first(md.get("creator")) or lead)
            if not by or by.lower() in ("various", "various artists", "va"):
                by = bp.clean(first(f.get("artist")) or lead or first(md.get("creator")))
            title = bp.clean(first(f.get("title")) or os.path.splitext(os.path.basename(f["name"]))[0].replace("_", " "))
            by, title = unmangle(by), unmangle(title)
            title = re.sub(r"^\d{1,3}\s*[.\-]\s*", "", title).strip()
            # A compilation's file named "07 Artist - Title" names its own artist.
            two = re.match(r"^\d{1,3}\s+(.+?) - (.+)$", title)
            if two and item in COMPILATIONS:
                by, title = two.group(1), two.group(2)
            elif two and item.startswith("InternetJukebox."):
                title = two.group(1)
            if item.startswith("InternetJukebox."):
                title = re.sub(r"^\d{1,3}\s+", "", title)
            title = TITLE.get(item, title)
            by = BY.get(by, by)
            if item.startswith("InternetJukebox."):
                by = "United States Marine Band"
            if title.lower().startswith(by.lower() + " - "):
                title = title[len(by) + 3:]
            if not by or not title or "Ã" in title + by:
                skipped.append(f"{item}/{f['name']}: no artist or title")
                continue
            # One release putting out the same track twice (a single, then an album) plays it once.
            if (title.lower(), round(bp.seconds(f.get("length")))) in twice:
                continue
            twice.add((title.lower(), round(bp.seconds(f.get("length")))))
            out.append(bp.entry(item, f, by[:60], title[:80], album, lic, name))
        print(f"{item}: {sum(1 for x in out if x['item'] == item)}", flush=True)
    return out


def books(lst):
    lst = [b for b in lst if b[0] not in HELD and free(bp.meta(b[0])["metadata"], b[0])]
    bp.CONTRABAND = lst
    out = bp.contraband()
    for item, by, work, _ in lst:
        mine = [x for x in out if x["item"] == item]
        w = 3 if len(mine) >= 100 else 2
        for i, x in enumerate(mine, 1):
            # Numbered by place, so the files sort in the order they are read. A section's name
            # stays only when it is a name: not a number, the book's own title again, or the
            # opening words of the chapter.
            m = re.match(rf"^{re.escape(work)}(?: \d+)?(?: \u00b7 (.+))?$", x["title"])
            rest = (m.group(1) or "") if m else ""
            if (not rest or re.fullmatch(r"[\d\s.]+", rest) or rest.lower().startswith(work.lower()) or rest[:1] in ":;,." or rest[:1].islower()
                    or rest.endswith("...") or len(rest.split()) > 8 or re.match(r"^(chapters?|section|part|book)\s+\d", rest, re.I)):
                rest = ""
            x["title"] = (f"{work} {i:0{w}d}" + (f" \u00b7 {rest}" if rest else ""))[:80] if len(mine) > 1 else work
    return out


def poems(have):
    seen = {(x["by"].lower(), x["title"].lower()) for x in have}
    out = []
    for item in POEMS:
        m = bp.meta(item)
        md = m["metadata"]
        got = free(md, item)
        if not got:
            continue
        lic, name = got
        album = bp.clean(first(md.get("title")))
        files = sorted({f["name"]: f for f in m["files"] if f["name"].endswith("_64kb.mp3")}.values(), key=lambda f: f["name"])
        # A poem in both of Blake's books keeps the book's name beside it; the rest go without.
        both = {t for t in (re.sub(r"\s*\((Innocence|Experience)\)$", "", bp.clean(first(f.get("title")))) for f in files)
                if sum(re.sub(r"\s*\((Innocence|Experience)\)$", "", bp.clean(first(g.get("title")))) == t for g in files) > 1}
        for f in files:
            by = bp.POET.get(bp.clean(first(f.get("artist")) or first(md.get("creator"))), bp.clean(first(f.get("artist")) or first(md.get("creator"))))
            title = re.sub(r"^\d{1,3}\s*[.\-]\s*", "", bp.clean(first(f.get("title")))).strip()
            title = re.sub(r"\s*(?:\((?:read by|reader)[^)]*\)|-\s*read by .*)$", "", title, flags=re.I)
            if re.sub(r"\s*\((Innocence|Experience)\)$", "", title) not in both:
                title = re.sub(r"\s*\((Innocence|Experience)\)$", "", title)
            # Blake's two books whole, in one file each: the poems come one by one from the other reading.
            if item == "poems_wm_blake_ss_librivox" and title.startswith("Songs of "):
                continue
            title = re.sub(r"\s+\d\d-\d\d$", "", title)
            if bp.RETITLE.get(title, "") is None:
                continue
            title = bp.RETITLE.get(title) or title
            k = (by.lower(), title.lower())
            if not title or k in seen:
                continue
            seen.add(k)
            out.append(bp.entry(item, f, by, title[:80], album, lic, name))
        print(f"{item}: {sum(1 for x in out if x['item'] == item)}", flush=True)
    return out


def unique(rows, kept=0):
    """A Lantern skips a recording whose title its station already has, so a new title that
    repeats one takes a number. The first `kept` rows are on the air already and keep theirs."""
    seen = {}
    for i, r in enumerate(rows):
        t = r["title"]
        if i >= kept and t in seen:
            n = seen[t] + 1
            while f"{t} ({n})" in seen:
                n += 1
            seen[t] = n
            r["title"] = f"{t} ({n})"
            seen[r["title"]] = 1
        else:
            seen.setdefault(t, 1)
    return rows


def key(x):
    return x.get("item") or x.get("url", "")


def write(name, rows, kept=0):
    rows = unique(rows, kept)
    json.dump(rows, open(f"{HERE}/{name}.json", "w", encoding="utf-8"), indent=1, ensure_ascii=False)
    print(f"{name}: {len(rows)} recordings, {sum(r['length'] for r in rows) / 3600:.1f} hours, {sum(r['size'] for r in rows) / 1e6:.0f} MB", flush=True)


if __name__ == "__main__" and sys.argv[1:] == ["poets"]:
    old = json.load(open(f"{HERE}/poets.json", encoding="utf-8"))[:174]
    write("poets", old + poems(old), len(old))
elif __name__ == "__main__":
    old = {n: json.load(open(f"{HERE}/{n}.json", encoding="utf-8")) for n in ("netlabel", "poets", "contraband", "shanties")}
    sh = old["shanties"]
    moved = [x for x in sh if key(x) in TO_BRASS]
    # The Marine Band's Sousa marches leave too: Brass has every one.
    kept = [x for x in sh if key(x) not in TO_BRASS and not key(x).startswith("InternetJukebox.") and not any(r in key(x) for r in RETIRE)]
    brass = music(BRASS)
    jps = {x["item"] for x in brass}
    write("brass", brass + [x for x in moved if x.get("item") not in jps])
    write("shanties", kept, len(kept))
    write("netlabel", old["netlabel"] + [x for x in music(NETLABEL) if x["item"] not in {y["item"] for y in old["netlabel"]}], len(old["netlabel"]))
    write("drift", music(DRIFT))
    write("concert-hall", music(CONCERT))
    write("public-domain", music(PUBLIC_DOMAIN))
    write("readings", books(READINGS))
    write("contraband", old["contraband"] + [x for x in books(CONTRABAND) if x["item"] not in {y["item"] for y in old["contraband"]}], len(old["contraband"]))
    write("poets", old["poets"] + poems(old["poets"]), len(old["poets"]))
    for s in skipped:
        print("left out:", s)
