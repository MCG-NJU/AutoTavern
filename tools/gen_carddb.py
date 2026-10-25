"""Generate the engine card database from the read-only resource bundle.

Output: crates/taverngym/data/cards.json (embedded into the Rust engine at compile time).
Run:    python tools/gen_carddb.py          (write)
        python tools/gen_carddb.py --check  (exit 1 if the committed file is stale)

Only game-relevant categories are exported; enchantments, cosmetics, anomalies and quest
objects stay in the bundle. Pool membership comes from rules/current_ruleset.json and the
current views -- never from the raw catalogue alone.

The resource bundle (a content pack compiled from the 253216 client data) is not distributed
with this repository; to regenerate, point `--bundle DIR` or `TAVERNGYM_BUNDLE` at a copy.
"""

import json
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BUNDLE = Path(os.environ.get("TAVERNGYM_BUNDLE", ROOT / "content_pack"))
OUT = ROOT / "crates" / "taverngym" / "data" / "cards.json"
# Patch overlay (36.6.3) on top of the read-only 253216 / 36.6.1 resource pack.
OVERLAY = ROOT / "tools" / "overlays" / "36.6.3.json"

KINDS = {
    "minion": "minion",
    "tavern_spell": "tavern_spell",
    "special_spell": "spell",
    "hero": "hero",
    "hero_power": "hero_power",
    "trinket": "trinket",
    "dark_gift": "dark_gift",
    "quest_or_quest_carrier": "quest",
    "quest_reward": "quest_reward",
}
RACES = ["BEAST", "DEMON", "DRAGON", "ELEMENTAL", "MECHANICAL", "MURLOC", "PIRATE",
         "QUILBOAR", "UNDEAD", "ABERRATION", "NAGA", "ALL"]
TAG_RE = re.compile(r"<[^>]+>")


def clean(text):
    t = TAG_RE.sub("", text or "").replace("[x]", "").replace("\xa0", " ")
    # A hyphenated word broken over two lines ("highest-\nAttack") is joined back with a
    # single space, any other line break becomes a space; runs of spaces collapse.
    t = t.replace("-\n", "- ").replace("\n", " ")
    return re.sub(r" {2,}", " ", t).strip()


def split_variants(text):
    """Card texts with a progress counter come as two variants glued together with the
    counter's value: "A (3 left!)3A (Done!)". Returns [first, second] ("" when single)."""
    head = text[:5]
    for m in re.finditer(r"\d+", text):
        # The second variant may itself start with a digit ("...2" + "2回合后...").
        for cut in range(m.end(), m.start(), -1):
            if m.start() >= 8 and text[cut:cut + 5] == head:
                return [text[:m.start()].strip(), text[cut:].strip()]
    return [text, ""]


def set_texts(c):
    """Split glued variants of `text` / `text_zh` into the shown text and `text_alt*`. The
    Chinese variant shown is the one whose numbers match the English text."""
    en, en_alt = split_variants(c["text"])
    zh, zh_alt = split_variants(c.get("text_zh") or "")
    nums = lambda t: re.findall(r"\d+", t)
    if zh_alt and nums(zh) != nums(en) and nums(zh_alt) == nums(en):
        zh, zh_alt = zh_alt, zh
    if zh_alt == zh:
        zh_alt = ""
    c["text"], c["text_zh"] = en, zh
    for k, v in (("text_alt", en_alt), ("text_alt_zh", zh_alt)):
        if v:
            c[k] = v
        else:
            c.pop(k, None)


def text_keywords(kind, kws, text):
    """Mechanic labels the bundle tags miss (a minion whose text has "Start of Combat:")."""
    kws = set(kws)
    if kind == "minion" and "Start of Combat:" in text:
        kws.add("START_OF_COMBAT")
    return sorted(kws)


def load_json(rel):
    return json.loads((BUNDLE / rel).read_text())


def build():
    catalogue = {}
    with open(BUNDLE / "data" / "catalogue.jsonl") as f:
        for line in f:
            o = json.loads(line)
            catalogue[o["id"]] = o

    spell_school = {}
    pool_tag = {}
    quest_goal = {}  # QUEST_PROGRESS_TOTAL (base quest / reward progress goal)
    prize_tier = {}  # BATTLEGROUNDS_DARKMOON_PRIZE_TURN (Darkmoon Prize tier 1-4)
    reward_dbf = {}  # BACON_CARD_DBID_REWARD: the card a Quest Reward refers to
    with open(BUNDLE / "data" / "raw_selected_entities.jsonl") as f:
        for line in f:
            o = json.loads(line)
            s = (o.get("enUS") or {}).get("spellSchool")
            if s:
                spell_school[o["id"]] = s
            pt = ((o.get("xml") or {}).get("tags") or {}).get("BATTLEGROUNDS_DARKMOON_PRIZE_TURN")
            if pt:
                prize_tier[o["id"]] = pt
            rd = ((o.get("xml") or {}).get("tags") or {}).get("BACON_CARD_DBID_REWARD")
            if rd:
                reward_dbf[o["id"]] = rd
            q = ((o.get("xml") or {}).get("tags") or {}).get("QUEST_PROGRESS_TOTAL")
            if q is not None:
                quest_goal[o["id"]] = q
            t = ((o.get("xml") or {}).get("tags") or {}).get("IS_BACON_POOL_MINION")
            if t is not None:
                pool_tag[o["id"]] = t
    reachable = set(load_json("data/views/explicit_reachable_from_current_roots.json")["ids"])

    ruleset = load_json("rules/current_ruleset.json")
    trinket_rules = load_json("rules/trinket_offering.json")
    gift_rules = load_json("rules/dark_gift_offering.json")

    pools = {
        "minion": set(ruleset["regular_pool_candidate_ids"]),
        "tier7": set(ruleset["tier7_special_ids"]),
        "hero": set(ruleset["hero_candidate_ids"]),
        "spell": set(ruleset["spell_candidate_ids"]),
        "trinket": set(trinket_rules["candidate_ids"]) - set(trinket_rules["temporary_ban_ids"]),
        # Timewarp minions that effects can still reach (Minor = Tier 3, Major = Tier 5):
        # not rotated-out Naga, not temporarily banned.
        "timewarp": set(load_json("data/views/timewarp_all_assets.json")["ids"])
        - set(load_json("data/views/naga_rotation_excluded.json")["ids"])
        - set(load_json("data/views/temporary_bans.json")["ids"]),
    }
    gift_windows = {}
    for g in gift_rules["gifts"]:
        w = g.get("offer_window") or {}
        gift_windows[g["card_id"]] = [
            w.get("minimum_effective_button_turn") or w.get("minimum_turn_raw"),
            w.get("maximum_turn_raw"),
        ]
    hero_power_of = {}
    buddy_of = {}  # hero -> its Buddy minion
    spellcraft_of = {}  # Spellcraft minion -> its temporary spell (BACON_SPELLCRAFT_ID)
    for o in catalogue.values():
        for r in o["relationships"]:
            if r["relation"] == "hero_power" and r["from"] == o["id"]:
                hero_power_of.setdefault(o["id"], r["to"])
            if r["relation"] == "buddy" and r["from"] == o["id"]:
                buddy_of.setdefault(o["id"], r["to"])
            if r["relation"] == "spellcraft_spell" and r["from"] == o["id"]:
                spellcraft_of.setdefault(o["id"], r["to"])

    dbf_to_id = {o["dbf_id"]: o["id"] for o in catalogue.values()}
    cards = []
    for cid, o in catalogue.items():
        kind = KINDS.get(o["category"])
        if kind is None:
            continue
        stats = o.get("stats") or {}
        cost = (o.get("cost") or {}).get("current_intended")
        if kind == "trinket":
            school = spell_school.get(cid, "")
            kind = "trinket_greater" if school == "GREATER_TRINKET" else "trinket_lesser"
        in_pool = []
        for name, ids in pools.items():
            if cid in ids:
                in_pool.append(name)
        entry = {
            "id": cid,
            "dbf": o["dbf_id"],
            "kind": kind,
            "name": o["name"].get("enUS") or cid,
            "name_zh": o["name"].get("zhCN") or "",
            "tier": o.get("tier") or 0,
            "atk": stats.get("attack") or 0,
            "hp": stats.get("health") or 0,
            "armor": stats.get("armor") or 0,
            "cost": cost if isinstance(cost, int) else 0,
            "races": [r for r in o.get("race") or [] if r in RACES],
            # Offer association (trinkets): battlegroundsAssociatedRaces.
            "assoc": [r for r in o.get("associated_races") or [] if r in RACES],
            "keywords": text_keywords(
                kind,
                o.get("keywords") or [],
                clean((o.get("text_current_intended") or {}).get("enUS")),
            ),
            "golden": o.get("golden_id") or "",
            "normal": o.get("normal_id") or "",
            "pools": in_pool,
            "text": clean((o.get("text_current_intended") or {}).get("enUS")),
            "text_zh": clean((o.get("text_current_intended") or {}).get("zhCN")),
            # Raw client IS_BACON_POOL_MINION value (0 = absent, 1 = normal; see RulesConfig).
            "pool_tag": pool_tag.get(cid, 0),
            # Explicitly reachable from current pool roots (tokens, generated cards...).
            "reachable": cid in reachable,
        }
        if kind == "hero":
            entry["hero_power"] = hero_power_of.get(cid, "")
            entry["buddy"] = buddy_of.get(cid, "")
        if cid in prize_tier:
            entry["prize_tier"] = prize_tier[cid]
        if cid in reward_dbf:
            entry["reward_card"] = dbf_to_id.get(reward_dbf[cid], "")
        if cid in quest_goal:
            entry["quest_goal"] = quest_goal[cid]
        if spell_school.get(cid) == "SPELLCRAFT":
            entry["spellcraft_spell"] = True
        if cid in spellcraft_of:
            entry["spellcraft"] = spellcraft_of[cid]
        if kind == "dark_gift":
            entry["window"] = gift_windows.get(cid, [None, None])
        cards.append(entry)
    cards.sort(key=lambda c: c["id"])
    meta = {
        "client_version": ruleset["client_version"],
        "server_patch": ruleset["server_patch"],
        "source": "content pack of client 253216 / 36.6.1 (generated by tools/gen_carddb.py)",
        "count": len(cards),
    }
    return {"meta": meta, "cards": cards}


def render(db):
    # One card per line: diff-friendly and still compact.
    lines = ['{"meta":' + json.dumps(db["meta"], ensure_ascii=False) + ',"cards":[']
    body = [json.dumps(c, ensure_ascii=False, separators=(",", ":")) for c in db["cards"]]
    lines.append(",\n".join(body))
    lines.append("]}")
    return "\n".join(lines) + "\n"


def apply_overlay(db, overlay):
    """Apply a patch overlay (tools/overlays/*.json) on top of the read-only resource pack: per-card field changes, pool changes, gift windows."""
    by_id = {c["id"]: c for c in db["cards"]}
    for cid, fields in overlay["cards"].items():
        c = by_id[cid]
        for k, v in fields.items():
            if k == "keywords_add":
                c["keywords"] = sorted(set(c["keywords"]) | set(v))
            elif k == "keywords_remove":
                c["keywords"] = sorted(set(c["keywords"]) - set(v))
            else:
                c[k] = v
    for pool, ch in overlay["pools"].items():
        for cid in ch.get("add", []):
            c = by_id[cid]
            if pool not in c["pools"]:
                c["pools"].append(pool)
            if pool == "minion":
                c["pool_tag"] = 1
            c["reachable"] = True
            if c["golden"]:
                by_id[c["golden"]]["reachable"] = True
        for cid in ch.get("remove", []):
            c = by_id[cid]
            c["pools"] = [x for x in c["pools"] if x != pool]
            if pool == "minion":
                c["pool_tag"] = 0
    db["meta"].update(overlay["meta"])
    return db


def finish(db):
    for c in db["cards"]:
        set_texts(c)
    return db


def main():
    global BUNDLE
    if "--bundle" in sys.argv:
        BUNDLE = Path(sys.argv[sys.argv.index("--bundle") + 1])
    if not (BUNDLE / "data" / "catalogue.jsonl").is_file():
        sys.exit(f"no resource bundle at {BUNDLE} (pass --bundle DIR or set TAVERNGYM_BUNDLE)")
    db = build()
    if OVERLAY.exists():
        db = apply_overlay(db, json.loads(OVERLAY.read_text()))
    db = finish(db)
    text = render(db)
    if "--check" in sys.argv:
        if not OUT.exists() or OUT.read_text() != text:
            print(f"{OUT} is stale; run python tools/gen_carddb.py")
            sys.exit(1)
        print("cards.json up to date")
        return
    OUT.write_text(text)
    print(f"wrote {OUT} ({len(text) // 1024} KiB)")


if __name__ == "__main__":
    main()
