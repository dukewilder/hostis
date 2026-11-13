#!/usr/bin/env python3
# Writes the manifests that fetch.py follows for HOSTIS's archive-built stations: every
# recording by its archive.org item and file (or its URL on another host), checksum, licence and
# credit, chosen once here so that the Lantern that keeps the station fetches exactly these and
# nothing else.
#   build_manifests.py            (retired: build_pass.py writes netlabel.json and poets.json)
#   build_manifests.py shanties   (writes shanties.json)
# No rights reserved.
import html, json, os, re, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))

# Netlabel: Creative Commons electronic, ambient and experimental, from long-running netlabels
# whose releases archive.org keeps with their licences.
NETLABEL = ["mtcomp001", "mtcomp002", "mtcomp003", "obcomp001", "NS050", "DWK031", "DWK315",
            "pcr089EmilDavydov-Sketches", "LBN003_-_Elisa_Luu_-_The_time_of_waiting", "afm004_allmyfaults_neonoir",
            "afm020_noctiflora_0407", "csr020", "Vkrsnl037CandlegravityAMomentForMyself"]
# Poets' Corner: LibriVox's short poetry collections, read by volunteers into the public domain.
POETS = ["short_poetry_001_librivox", "shortpoetry_002_librivox", "shortpoetry_005_librivox", "shortpoetry_043_librivox",
         "short_poetry_collection_082_librivox", "spc276_2606_librivox", "spc277_2607_librivox"]

# Shanties: sea shanties, sea songs, pirate songs and hornpipes from 78s, cylinders and the
# Library of Congress's National Jukebox, every one published in 1925 or earlier and so in the
# public domain in the United States.
SHANTIES = [
    '78_medley-of-sea-chanties_standard-male-quartette_gbia0381679a',
    'loc-jukebox-133664-medley-of-sea-songs',
    '78_rio-grande-sea-shanty-billy-boy-sea-shanty-songs-from-the-week-end-book_gos',
    '78_shenandoah-sea-shanty-songs-from-the-week-end-book_goss-john-cathedral-male',
    '78_rocked-in-the-cradle-of-the-deep_haydn-quartet_gbia0299467a',
    'edison-80072_01_1286',
    '78_rocked-in-the-cradle-of-the-deep_harry-lauder-knight_gbia3036767a',
    'WilliamFHooley',
    'edison-50300_01_4017',
    '78_asleep-in-the-deep_charles-laird-petrie-lamb_gbia3014455b',
    'loc-jukebox-646529-a-life-on-the-ocean-wave',
    'loc-jukebox-126354-the-bay-of-biscay',
    '78_nancylee_250_08',
    '78_nancy-lee_charles-harrison-stephen-adams_gbia0444944a',
    'edison-80155_01_2639',
    '78_anchored_thomas-chalmers-michael-watson_gbia0173677b',
    '78_anchored_kenneth-walters-cowan-watson_gbia3038201b',
    '78_sons-of-the-sea_mr-harold-buxton-f-mcglennon_gbia3014626b',
    '78_the-indifferent-mariner_frank-c-stanley_gbia0297917b',
    'edison-80804_01_4010',
    'loc-jukebox-717219-the-sea-is-the-home-for-me',
    '78_the-treasures-of-the-sea-are-buried-deep_j-w-myers_gbia0549053a',
    '78_i-love-to-be-a-sailor_harry-lauder-lauder_gbia3045275a',
    '78_the-jolly-sailors_harlan-and-porter-israel-porter_gbia0438601b',
    '78_davy-jones-locker_wilfred-glenn-petrie_gbia0304801a',
    'edison-80716_01_6990',
    '78_shipmates-o-mine_robert-radford-sanderson_gbia0243141a',
    'loc-jukebox-69598-shipmates-o-mine',
    '78_ben-the-bosun_mr-harry-reynolds-stephen-adams_gbia0318660a',
    '78_the-sailors-dance_kenneth-walters-j-l-molloy_gbia3030644a',
    '78_captain-mac_john-thorne-wilfred-sanderson_gbia3012549b',
    '78_captain-strattons-fancy_reinald-werrenrath-john-masefield-deems-taylor_gbia0040360b',
    '78_the-anchors-weighed-march_peter-and-daniel-wyper_gbia3036507a',
    'loc-jukebox-428286-the-sailor-s-prayer',
    'edison-82314_01_9188',
    '78_viking-song-there-are-steel-ships-wanted-on-the-sea_emilio-de-gogorza-david-m_gbia0055438a',
    'loc-jukebox-67371-bells-of-the-sea',
    'loc-jukebox-122001-when-the-bell-in-the-lighthouse-rings',
    '78_the-ship-that-never-returned_vernon-dalhart-and-company-henry-c-work_gbia0176570a',
    '78_gwraig-y-morwr-the-sailors-wife_miss-winifred-lewis-r-davies-dr-joseph-parry_gbia3002396b',
    'loc-jukebox-654579-row-well-ye-mariners',
    '78_pirate-song_reinald-werrenrath-robert-louis-stevenson-henry-f-gilbert_gbia0040360a',
    'david-bispham-henri-f.-gilbet-pirate-song-columbia-tricolor-a-5778',
    '78_pirate-dreams_sophie-braslau-charles-huerter_gbia0484192a',
    '78_the-pirates-of-penzance-part-i_the-band-of-hm-scots-guards-sullivan_gbia3015847a',
    '78_the-pirates-of-penzance-part-ii_the-band-of-hm-scots-guards-sullivan_gbia3015847b',
    'edison-80251_01_3905',
    'edison-80251_01_3906',
    'loc-jukebox-129632-gems-from-pirates-of-penzance',
    'hmsp22',
    '78_gems-from-pinafore-part-1_victor-light-opera-company-gilbert-sullivan_gbia7027983a',
    '78_gems-from-pinafore-part-2_victor-light-opera-company_gbia7027983b',
    'edison-80169_01_2992',
    'edison-80169_01_2993',
    'loc-jukebox-128850-flying-dutchman-overture',
    '78_flying-dutchman-traft-ihr-das-schiff-a-ship-the-restless-ocean-sweeps-sentas_gbia0253553a',
    'loc-jukebox-666076-sailors-hornpipe-arranged',
    '78_the-sailors-hornpipe_george-ackroyd_gbia0337319b',
    '78_jack-tar-march_sousas-band_gbia0011119a',
    'loc-jukebox-124836-ship-ahoy-march',
    'loc-jukebox-248883-blue-jackets-march',
    'loc-jukebox-250383-sailing',
    '78_hornpipe-medley_mr-alexander-prince_gbia3037857a',
    'loc-jukebox-649564-medley-of-sea-chanties',
    'loc-jukebox-817646-sailor-song',
    'loc-jukebox-646454-ahoy-my-lads',
    'loc-jukebox-126340-the-death-of-nelson',
    'loc-jukebox-120790-the-salt-of-the-sea-for-me',
    '78_the-lads-in-navy-blue_mr-harold-buxton-h-dacre_gbia3014626a',
    'loc-jukebox-133475-sailing-down-the-chesapeake-bay',
    'edison-80186_01_3320',
    '78_larboard-watch_sydney-coltham-and-peter-dawson-t-williams_gbia7025400a',
    'loc-jukebox-133012-the-diver',
    '78_rolling-down-to-rio_herbert-witherspoon-rudyard-kipling-edward-german_gbia0253822a',
    '78_three-fishers_arthur-middleton-john-hullah_gbia0331528b',
    '78_a-rescue-by-the-life-boat-crew_standard-male-quartette_gbia0297926a',
    'loc-jukebox-650041-columbia-the-gem-of-the-ocean',
    '78_song-of-the-viking-guest_feodor-chaliapin-rimsky-korsakov_gbia0276267a',
    '78_l-africana-all-erta-marinar-what-ho-mariners_titta-ruffo-meyerbeer_gbia0196322a',
    '78_cielo-e-mar-heaven-and-ocean_enrico-caruso-ponchielli_gbia0565987a',
    '78_recitoftheoceanaria_145_14',
    '78_death-of-nelson-march_peter-and-daniel-wyper_gbia3036507b',
    'edison-50995_01_5617',
    'loc-jukebox-42208-anchors-aweigh',
    '78_sailors-hornpipe_r-murchie_gbia0057027b',
    'edison-51382_01_9261',
    '78_highland-fling-the-keelrow-sailors-hornpipe-college-hornpipe_london-dance',
]

# Shanties beyond 1925, free by other roads, each checked by hand: what the armed forces' own
# bands recorded (a work of the U.S. Government, 17 U.S.C. 105), and what their makers put under
# a public-domain dedication or a Creative Commons licence that needs no more than credit. The
# item's licence on archive.org must still say what "lic" expects, or the build stops.
# Left out on review: Captain Mac (an arrangement not shown to be the Navy's own), Captain Kidd
# (its licence was set by the venue, not the singer), The COVID Shanty (not the station's sound).
SHANTIES_FREE = [
    dict(item="RoastBeefOfEngland", file="RoastBeefOfEngland.mp3", lic="publicdomain", by="United States Navy Band Ceremonial Band", title="The Roast Beef of Old England",
         album="the Navy Band's ceremonial music", licence="Public domain in the United States: a work of the U.S. Navy Band, made by sailors in the course of their duty."),
    *[dict(item=f"InternetJukebox.JPS.{n}", file=f, lic="publicdomain", by="United States Marine Band", title=t,
           album="The Complete Marches of John Philip Sousa", licence=f"Public domain in the United States: a work of the U.S. Marine Band, made in the course of its duty; Sousa wrote the march in {y}.")
      for n, f, t, y in [(83, "83_The_Chantyman's_March.mp3", "The Chantyman's March", 1918), (81, "81_Anchor_and_Star.mp3", "Anchor and Star", 1918),
                         (62, "01 Jack Tar - US Marine Corps.mp3", "Jack Tar", 1903), (56, "56_Hands_Across_The_Sea.mp3", "Hands Across the Sea", 1899),
                         (67, "67_The_Glory_of_the_Yankee_Navy.mp3", "The Glory of the Yankee Navy", 1909), (77, "77_The_Naval_Reserve.mp3", "The Naval Reserve", 1917),
                         (93, "93_Who's_Who_in_the_Navy_Blue.mp3", "Who's Who in Navy Blue", 1920), (57, "57_The_Man_Behind_the_Gun.mp3", "The Man Behind the Gun", 1899)]],
    dict(item="HangingJohnny", file="HangingJohnny.mp3", lic="/by/", by="Jeff Carter", title="Hanging Johnny", album="a 2009 recording",
         licence="CC BY 3.0 US: Jeff Carter, https://archive.org/details/HangingJohnny"),
    dict(item="jamendo-381147", file="01-1841866-ebunny-Wellerman_s Apple _Sea Shanty Guitar_.mp3", lic="/by/", by="ebunny", title="Wellerman's Apple (Sea Shanty Guitar)",
         album="a 2021 recording", licence="CC BY 3.0: ebunny, https://www.jamendo.com/album/381147"),
]

# Shanties from hosts other than archive.org, fetched by URL. The Library of Congress's own
# WPA California Folk Music Project recordings (1938 to 1940), which say they were made by a
# field researcher employed by the U.S. Government, and Wikimedia Commons uploads under CC BY or
# CC BY-SA. The Library of Congress gives no checksum, so the fetch checks those by size.
WPA = "Public domain in the United States: recorded for the WPA California Folk Music Project by Sidney Robertson Cowell, a field researcher employed by the U.S. Government; the Library of Congress knows of no copyright in it."
CREW = "Leighton Robinson, Alex Barr, Arthur Brodeur and Leighton McKenzie"
SHANTIES_URLS = [
    *[dict(url=f"https://tile.loc.gov/storage-services/media/afc/afc1940001/afc1940001_{f}.mp3", size=s, length=round(s / 40000), by=b, title=t,
           album=f"a {y} field recording", licence=WPA, source=f"https://www.loc.gov/item/{lccn}/")
      for lccn, f, s, b, t, y in [
          ("2017701730", "a4229a1", 11756160, CREW, "Goodby, Fare You Well", 1939),
          ("2017701731", "a4229b1", 8522880, CREW, "Poor Old Man", 1939),
          ("2017701732", "a4230a1", 10993920, CREW, "Rolling Home", 1939),
          ("2017701733", "a4230b1", 11990400, CREW, "Sailor's Alphabet", 1939),
          ("2017701734", "a4231a1", 5142720, CREW, "Hanging Johnny", 1939),
          ("2017701735", "a4231a2", 6935040, CREW, "Sally Brown", 1939),
          ("2017701736", "a4232a1", 9628800, CREW, "Away, Rio", 1939),
          ("2017701737", "a4232b1", 5507520, CREW, "Whiskey Johnny", 1939),
          ("2017701738", "a4232b2", 4602240, CREW, "Roll the Cotton Down", 1939),
          ("2017701729", "a4228a2", 8220480, "Leighton Robinson", "A-Roodle-Tum-Toodle-Tum-Too", 1939),
          ("2017701408", "a3812b1", 6938880, "George Vinton Graham", "Down Around the Coast of La Barbaree", 1938),
          ("2017701396", "a3810a1", 4957440, "George Vinton Graham", "Her Sweet Sailor Boy", 1938),
          ("2017701394", "a3816b1", 5967360, "George Vinton Graham", "Young Johnny the Sailor", 1938),
          ("2017701407", "a3812a2", 6810240, "George Vinton Graham", "Lowlands of Holland", 1938),
          ("2017701454", "a4206b1", 9568320, "Pat Ford", "The Dark-Eyed Sailor", 1938),
          ("2017701809", "a4211b1", 6019200, "Pat Ford", "The Sailor's Grave", 1939),
          ("2017701439", "a4202a4", 2029440, "Warde Ford", "Proud Dacus and Captain Hull", 1938),
          ("2017701429", "a4199a2", 5057280, "Warde Ford", "The Mermaid", 1938),
          ("2017702043", "a3358a1", 2620800, "John Stone", "Life on the Ocean Wave", 1939),
          ("2017701371", "a3354b1", 11465280, "John McCready", "Sailor Jack", 1939)]],
    *[dict(url="https://upload.wikimedia.org/wikipedia/commons/" + u, sha1=h, size=s, length=n, by=b, title=t, album=f"a {y} recording",
           licence=f"{lic}: {credit}, https://commons.wikimedia.org/wiki/File:{p}", source=f"https://commons.wikimedia.org/wiki/File:{p}")
      for u, p, h, s, n, b, t, y, lic, credit in [
          ("9/9e/Haul_Away%2C_Joe.ogg", "Haul_Away,_Joe.ogg", "62c3ff2a9ac46058ba584d80d0ef59fbf7245916", 687049, 65, "Shanty Crew X-berg", "Haul Away, Joe", 2012,
           "CC BY-SA 3.0", "Shanty Crew X-berg, recorded by Uwe.Rixdorf"),
          ("f/f3/College_Hornpipe.ogg", "College_Hornpipe.ogg", "92daed49fe1d9e486c70c128261c88313bbef8c0", 700953, 46, "Kildwyke", "College Hornpipe", 2011,
           "CC BY-SA 3.0", "Kildwyke"),
          ("b/be/Netherworld_Shanty_%28ISRC_USUAN1100138%29.mp3", "Netherworld_Shanty_(ISRC_USUAN1100138).mp3", "88ac27f997ca593be572fcdcb2e464c404f34547", 7937600, 195,
           "Kevin MacLeod", "Netherworld Shanty", 2007, "CC BY 3.0", "Kevin MacLeod (incompetech.com)"),
          ("c/c7/Leis_a_Lurrighan.ogg", "Leis_a_Lurrighan.ogg", "90af960f6fb3c0934c0c4d289fabf0806bf77b45", 3849905, 167, "Ceili Moss", "Leis a Lurrighan", 2006,
           "CC BY 3.0", "Ceili Moss")]],
]


def meta(item):
    with urllib.request.urlopen(f"https://archive.org/metadata/{item}", timeout=60) as r:
        return json.load(r)


def seconds(v):
    if not v:
        return 0
    if ":" in str(v):
        p = [float(x) for x in str(v).split(":")]
        return sum(x * 60 ** i for i, x in enumerate(reversed(p)))
    return float(v)


def clean(s):
    s = re.sub(r"\s+", " ", html.unescape(str(s or ""))).strip()
    # A Lantern's seed folder is on Linux: only a slash cannot stand in a file name.
    return s.replace(chr(92), "").replace("/", "-").replace(chr(0), "")[:90]


def take(items, kind, max_tracks, min_s, max_s):
    out = []
    for item in items:
        m = meta(item)
        md = m["metadata"]
        lic = md.get("licenseurl", "")
        if kind == "music" and "creativecommons.org" not in lic:
            print("skip, no CC licence:", item)
            continue
        if kind == "poems" and "publicdomain" not in lic:
            print("skip, not public domain:", item)
            continue
        album = clean(md.get("title"))
        album_by = clean(md.get("creator") if isinstance(md.get("creator"), str) else (md.get("creator") or [""])[0])
        files = [f for f in m["files"] if f.get("source") == "original" and f["name"].lower().endswith(".mp3")]
        if kind == "poems":
            files = [f for f in m["files"] if f["name"].endswith("_64kb.mp3")]
        n = 0
        for f in sorted(files, key=lambda f: f["name"]):
            length = seconds(f.get("length"))
            if not (min_s <= length <= max_s):
                continue
            title = clean(f.get("title")) or clean(os.path.splitext(f["name"])[0])
            by = clean(f.get("artist") or f.get("creator") or album_by)
            by = re.sub(r"-monotonik$", "", by)
            # Titles as files were named: underscores, track numbers and the artist's own name.
            title = re.sub(r"^\d{1,3}\.\s*", "", title.replace("_", " ")).strip()
            title = re.sub(r"^\d{1,3}\s+-\s+", "", title).strip()
            title = re.sub(r"\s{2,}", " ", title)
            for lead in (by, by.replace("-", " ")):
                if title.lower().startswith(lead.lower() + " - "):
                    title = title[len(lead) + 3:]
            title = re.sub(r"^\d{1,3}\s+-\s+", "", title).strip()
            if not title or "\u00c3" in title or "\u00c3" in by:
                continue
            if not by or by.lower() in ("various", "various artists", "none"):
                continue
            out.append({"item": item, "file": f["name"], "sha1": f.get("sha1"), "size": int(f.get("size", 0)), "length": round(length),
                        "by": by, "title": title, "album": album, "licence": lic})
            n += 1
            if n >= max_tracks:
                break
        print(f"{item}: {n} recordings")
    return out

# What a label or catalogue got wrong or left awkward: the performer in place of a composer,
# titles out of capitals, one take where an item holds two.
FIX = {
    "loc-jukebox-133664-medley-of-sea-songs": {"by": "Victor Mixed Chorus"},
    "loc-jukebox-126354-the-bay-of-biscay": {"by": "Harold Jarvis"},
    "loc-jukebox-69598-shipmates-o-mine": {"by": "Royal Dadmun"},
    "loc-jukebox-122001-when-the-bell-in-the-lighthouse-rings": {"by": "Frank C. Stanley"},
    "loc-jukebox-654579-row-well-ye-mariners": {"by": "Prince's Band", "title": "Row Well, Ye Mariners"},
    "loc-jukebox-129632-gems-from-pirates-of-penzance": {"by": "Victor Light Opera Company"},
    "loc-jukebox-128850-flying-dutchman-overture": {"by": "Arthur Pryor's Band", "title": "The Flying Dutchman, Overture"},
    "loc-jukebox-124836-ship-ahoy-march": {"by": "Arthur Pryor's Band", "title": "Ship Ahoy March"},
    "loc-jukebox-248883-blue-jackets-march": {"by": "Arthur Pryor's Band", "title": "Blue Jackets March"},
    "loc-jukebox-666076-sailors-hornpipe-arranged": {"title": "Sailor's Hornpipe"},
    "hmsp22": {"by": "Frederic Ranalow and company"},
    "78_flying-dutchman-traft-ihr-das-schiff-a-ship-the-restless-ocean-sweeps-sentas_gbia0253553a": {"title": "Senta's Ballad, from The Flying Dutchman"},
    "loc-jukebox-649564-medley-of-sea-chanties": {"by": "Peerless Quartet"},
    "loc-jukebox-817646-sailor-song": {"by": "Peerless Quartet"},
    "loc-jukebox-646454-ahoy-my-lads": {"by": "Fred Van Eps and Vess L. Ossman"},
    "loc-jukebox-133475-sailing-down-the-chesapeake-bay": {"by": "American Quartet"},
    "loc-jukebox-42208-anchors-aweigh": {"by": "United States Marine Band"},
    "loc-jukebox-650041-columbia-the-gem-of-the-ocean": {"title": "Columbia, the Gem of the Ocean"},
    "edison-51382_01_9261": {"by": "Jep Bisbee and Beulah Bisbee-Schuler"},
    "78_sailors-hornpipe_r-murchie_gbia0057027b": {"title": "Sailor's Hornpipe"},
    "78_highland-fling-the-keelrow-sailors-hornpipe-college-hornpipe_london-dance": {"title": "Highland Fling; Sailor's Hornpipe"},
    "78_l-africana-all-erta-marinar-what-ho-mariners_titta-ruffo-meyerbeer_gbia0196322a": {"title": "All'erta, Marinar! (What Ho, Mariners!), from L'Africana"},
    "78_cielo-e-mar-heaven-and-ocean_enrico-caruso-ponchielli_gbia0565987a": {"title": "Cielo e Mar (Heaven and Ocean)"},
    "78_recitoftheoceanaria_145_14": {"by": "Anna Bahr-Mildenburg", "title": "Ocean, Thou Mighty Monster, from Oberon"},
}
# Items that hold two copies of one side: the first is taken.
ONE = ("78_nancylee", "78_recitoftheoceanaria")
TITLES = [("THE PIRATES OF PENZANCE; Part", "The Pirates of Penzance, Part"), ("Gems from Pinafore-Part", "Gems from Pinafore, Part"),
          ("Rio Grande (sea shanty); Billy Boy (sea shanty)", "Rio Grande; Billy Boy"), ("Shenandoah (Sea Shanty)", "Shenandoah"),
          ('Viking Song ("There are Steel Ships Wanted on the Sea")', "Viking Song"), ("HMS Pinafore (English Operetta), Act", "H.M.S. Pinafore, Act"),
          ("Flying Dutchman - Traft ihr das Schiff (A Ship the Restless Ocean Swee", "Senta's Ballad, from The Flying Dutchman")]


def proper(s):
    """Titles and names from 78 labels, which are often in capitals or written surname first."""
    s = clean(s)
    if "," in s and len(s.split(",")) == 2 and len(s) < 40:
        last, first = [x.strip() for x in s.split(",")]
        s = f"{first} {last}"
    if s.isupper() or s.islower():
        small = {"a", "an", "and", "at", "by", "for", "from", "in", "of", "on", "or", "the", "to", "with", "o'"}
        w = s.lower().split()
        s = " ".join(x if (i and x in small) else x[:1].upper() + x[1:] for i, x in enumerate(w))
    return s.strip(" .;")


def titled(t):
    """One way of capitalising across the station, whatever each label did."""
    small = {"a", "an", "and", "at", "by", "for", "from", "in", "of", "on", "or", "the", "to", "with", "o'", "is", "are"}
    words = t.rstrip(".").split()
    out = []
    for i, w in enumerate(words):
        lw = w.lower()
        if w.isupper() and (len(w) <= 4 and "." in w or w in ("I", "II", "H.M.S.")):
            out.append(w)
        elif w.startswith("("):
            out.append("(" + w[1:2].upper() + w[2:])
        elif i and lw in small and not out[-1].endswith((",", ";", ":")):
            out.append(lw)
        else:
            out.append(w[:1].upper() + w[1:].lower() if w.isupper() or w.istitle() or w.islower() else w)
    return " ".join(out)


def take_pd(items, cutoff=1925):
    out = []
    for item in items:
        m = meta(item)
        md = m["metadata"]
        year = int(str(md.get("date", "9999"))[:4] or 9999)
        if year > cutoff:
            print("skip, published after", cutoff, item)
            continue
        cr = md.get("creator") or ""
        cr = cr[0] if isinstance(cr, list) else cr.split(";")[0]
        by = re.sub(r"^(Mr|Mrs|Miss|Mme|Mlle)\.?\s+", "", proper(cr) or "Unknown")
        by = proper(by.title()) if by.isupper() or any(w.isupper() and len(w) > 3 for w in by.split()) else by
        by = by.replace("H.m.", "H.M.")
        files = [f for f in m["files"] if f["name"].lower().endswith(".mp3") and 45 <= seconds(f.get("length")) <= 3000]
        if item.startswith(ONE):
            files = files[:1]
        many = len(files) > 1
        for f in sorted(files, key=lambda f: f["name"]):
            title = clean(md.get("title") if isinstance(md.get("title"), str) else (md.get("title") or [""])[0])
            title = title if not (title.isupper() or title.islower()) else proper(title)
            title = title.split(" - Songs from")[0].split(" - Gilbert & Sullivan")[0]
            if many:
                title = f"{title}, {proper(os.path.splitext(f['name'])[0]).split()[-2:][0].title()} {os.path.splitext(f['name'])[0].split()[-1]}"
            for a, b in TITLES:
                title = title.replace(a, b)
            if item.startswith("david-bispham"):
                by, title = "David Bispham", "Pirate Song"
            fix = FIX.get(item, {})
            out.append({"item": item, "file": f["name"], "sha1": f.get("sha1"), "size": int(f.get("size", 0)), "length": round(seconds(f.get("length"))),
                        "by": fix.get("by", by)[:60], "title": (fix["title"] if "title" in fix else titled(title))[:80], "album": f"a {year} recording",
                        "licence": f"Public domain in the United States: published in {year}.", "source": f"https://archive.org/details/{item}"})
    return out


def take_free(entries):
    out = []
    for e in entries:
        m = meta(e["item"])
        lic = m["metadata"].get("licenseurl", "")
        if e["lic"] not in lic or "-nc" in lic or "-nd" in lic:
            raise SystemExit(f"{e['item']}: licence is now {lic!r}, not {e['lic']!r}")
        f = next(f for f in m["files"] if f["name"] == e["file"])
        out.append({"item": e["item"], "file": e["file"], "sha1": f.get("sha1"), "size": int(f.get("size", 0)), "length": round(seconds(f.get("length"))),
                    "by": e["by"], "title": e["title"], "album": e["album"], "licence": e["licence"], "source": f"https://archive.org/details/{e['item']}"})
    return out


if __name__ == "__main__" and "shanties" in os.sys.argv:
    sh = take_pd(SHANTIES) + take_free(SHANTIES_FREE) + SHANTIES_URLS
    json.dump(sh, open(f"{HERE}/shanties.json", "w"), indent=1, ensure_ascii=False)
    print(f"shanties: {len(sh)} recordings, {sum(x['length'] for x in sh) / 3600:.1f} hours, {sum(x['size'] for x in sh) / 1e6:.0f} MB")
elif __name__ == "__main__":
    # Netlabel's first list was CC BY-NC and BY-ND, which HOSTIS does not air, and Poets' Corner's
    # credits have been corrected since: build_pass.py writes both now.
    os.sys.exit("netlabel.json and poets.json come from build_pass.py")
    net = take(NETLABEL, "music", 9, 120, 900)
    json.dump(net, open(f"{HERE}/netlabel.json", "w"), indent=1, ensure_ascii=False)
    poems = take(POETS, "poems", 40, 20, 600)
    json.dump(poems, open(f"{HERE}/poets.json", "w"), indent=1, ensure_ascii=False)
    for name, l in [("netlabel", net), ("poets", poems)]:
        print(f"{name}: {len(l)} recordings, {sum(x['length'] for x in l) / 3600:.1f} hours, {sum(x['size'] for x in l) / 1e6:.0f} MB")
