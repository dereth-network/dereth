"""Build the length-keyed pseudonym list for the corpus scrubber.

Single-word names are hand-written (vague fantasy, no real IP); multi-word names are composed from
them. Every entry is validated for exact byte length. Output: pseudonyms.json keyed by length.
"""
import json, itertools, random, sys

SINGLE = {
 2: ["Ao","Ix","Um","Ys","Ek","Ov"],
 3: ["Ash","Bex","Cyr","Dov","Eli","Fen","Ivo","Kae","Lir","Mox","Nym","Orm","Pip","Rue","Syl","Tam","Ulf","Vex","Wyn","Zed"],
 4: ["Aren","Bryn","Cael","Dara","Ewin","Faye","Garr","Hale","Isla","Jory","Kael","Lyra","Mira","Nash","Orin","Pell","Quin","Rook","Sael","Thea","Ulla","Vane","Wren","Yara","Zora"],
 5: ["Aldis","Brann","Caius","Delva","Eirik","Fenna","Galen","Hesta","Idris","Jarek","Kiera","Lorne","Maren","Nerys","Osric","Perin","Rowan","Selka","Tarin","Ulric","Varen","Wynne","Yorik","Zarek"],
 6: ["Aldric","Briony","Corvin","Delwyn","Elowen","Farran","Gareth","Halvar","Isolde","Jareth","Kestra","Lorcan","Maelis","Nerine","Orland","Perrin","Quilla","Rosvin","Sorrel","Talwyn","Ulmara","Varick","Wilder","Yseult","Zephyr"],
 7: ["Aldwyne","Brannoc","Caedwyn","Dunstan","Elspeth","Fenwick","Gwendal","Hadrian","Isengar","Jorvath","Kerrick","Lysande","Morwenn","Nyssara","Orlaith","Pendrel","Quillon","Ravenna","Sylvane","Torvald","Ulrikke","Vesperi","Wyndham","Yestril","Zandrel"],
 8: ["Aldemere","Brenwick","Caerlith","Dorsvane","Eldrenne","Faelwynn","Galdorin","Hollowen","Isbrande","Jessamyn","Kaldrith","Lisandre","Merrowin","Nyrissae","Orvander","Peregrin","Quenlith","Rosamund","Sarelith","Thessaly","Ulvarden","Varendel","Wrenhold","Ysandrel","Zorander"],
 9: ["Aldervane","Brightmor","Caerwynne","Draventhe","Elisandre","Fallowmar","Greywater","Hallowden","Ismerelda","Jorunhild","Kaelithor","Lorimonde","Maristane","Nerrivane","Oakenhart","Pellamore","Quillfane","Ravenmoor","Sableford","Thornwick","Umberdell","Vanterose","Wintermar","Yarrowden","Zephyrine"],
 10: ["Aldermoore","Brackenrue","Cinderwyne","Duskmantle","Emberholde","Fernsworth","Glimmerash","Hollowmere","Ivorywinde","Juniperwyn","Kestrelfen","Lanternfen","Marrowgate","Nightbloom","Oakenshade","Pinemantle","Quietwater","Ravenshold","Silverfern","Thistledew","Umbergrove","Violetmoor","Willowmere","Yewhallows","Zephyrdale"],
 11: ["Amberhollow","Brightwater","Cobblestorm","Duskwhisper","Elderbranch","Frostmantle","Glassmeadow","Heatherwind","Ivorythorne","Juniperdell","Kestrelmoor","Lanternvale","Mistgarland","Nettlebrook","Oakenharrow","Pinewhisper","Quillhollow","Rivermantle","Silverbrook","Thornhallow","Umberwaters","Velvetgrove","Wolfsbriars","Yarrowfield","Zephyrwater"],
 12: ["Ashenwhisper","Bramblewater","Cinderhallow","Dawnwatchers","Emberwhistle","Frostwhisper","Gloamingdell","Harrowmantle","Ironwhistler","Juniperwater","Kindlewander","Lanternmoors","Marrowhallow","Nightshimmer","Oakenwhisper","Pebblewander","Quillwanders","Ravenwhisper","Silvermantle","Thistlewater","Umberwhisper","Violetmantle","Willowwander","Yarrowmantle","Zephyrhallow"],
}
EPITHETS = ["the Grey","the Quiet","of Ashvale","of the Fen","Ironhand","Stormborne","the Wanderer","of Hollowmere","Nightwarden","the Unbowed"]

def main(out):
    bad = [(L, w) for L, ws in SINGLE.items() for w in ws if len(w.encode()) != L]
    if bad:
        print("LENGTH ERRORS:", bad); sys.exit(1)
    dup = [w for ws in SINGLE.values() for w in ws]
    assert len(dup) == len(set(dup)), "duplicate single-word name"
    rng = random.Random(20260917)
    table = {L: list(ws) for L, ws in SINGLE.items()}
    # two-word names for 13..25, three-word for 26..32; at least 8 per length
    singles = [w for L, ws in SINGLE.items() if L >= 3 for w in ws]
    for L in range(13, 33):
        got = table.setdefault(L, [])
        tries = 0
        while len(got) < 8 and tries < 20000:
            tries += 1
            if L <= 25:
                a, b = rng.choice(singles), rng.choice(singles)
                name = f"{a} {b}"
            else:
                a, b = rng.choice(singles), rng.choice(singles)
                name = f"{a} {b} {rng.choice(EPITHETS)}"
            if len(name.encode()) == L and name not in got and a != b:
                got.append(name)
        if len(got) < 8:
            print(f"warning: only {len(got)} names of length {L}")
    doc = {
        "_about": "Length-keyed pseudonyms for the corpus scrubber. Same byte length as the real name; assigned in order of first appearance per session; the real->pseudonym map stays private.",
        "characters": {str(L): table[L] for L in sorted(table)},
        "accounts": {"rule": "for a real account of length L: 'acct' + zero-padded ordinal to fill L (L>=5); 'ac'+ordinal for L 3-4; 'a'+ordinal for L 2; lower-case because the client lower-cases account names"},
        "passwords": {"rule": "for a real password of length L: the first L bytes of 'passwordpasswordpasswordpassword' (max 32); one filler for all, since nothing in the corpus keys on the password after login"},
    }
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        json.dump(doc, f, indent=1, ensure_ascii=True)
    print("lengths:", {L: len(v) for L, v in sorted(table.items())})

if __name__ == "__main__":
    main(sys.argv[1])
