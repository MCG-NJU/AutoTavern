"""Text rendering of player-visible states for TavernBench prompts."""

from __future__ import annotations

SYSTEM = (
    "You are an expert Hearthstone Battlegrounds player (patch 36.6.3, solo mode). "
    "Use only the rules given in the prompt. Answer with a single JSON object in the "
    "requested format and nothing else."
)

RULES_PRIMER = """## Game rules (summary)
- Recruit phase: buying a minion costs 3 Gold, selling gives 1 Gold, refreshing the tavern costs 1 Gold.
  Hand holds at most 10 cards, the warband at most 7 minions. Upgrading the tavern costs the shown price,
  which drops by 1 each turn you do not upgrade.
- Three copies of a minion merge into a golden copy (double stats); playing it lets you discover a
  minion from one tavern tier higher.
- Combat: the player with more minions attacks first (random if equal). Minions attack from left to
  right; each attack targets a random enemy minion (Taunt minions first). Attacker and target damage
  each other at the same time.
- Keywords: Divine Shield (the first damage is ignored), Venomous (destroys any minion it damages,
  once), Reborn (returns with 1 Health the first time it dies), Taunt, Windfury (attacks twice),
  Cleave (also hits the minions next to the target), Deathrattle (when it dies), Avenge (X) (after X
  friendly minions die), Start of Combat.
- The loser takes damage equal to the winner's tavern tier plus the tiers of the winner's surviving
  minions (capped while more than four players are alive: 5 before turn 4, 10 before turn 8, then 15).
"""

KW = {
    "DIVINE_SHIELD": "Divine Shield", "TAUNT": "Taunt", "VENOMOUS": "Venomous",
    "POISONOUS": "Poisonous", "REBORN": "Reborn", "WINDFURY": "Windfury",
    "MEGA_WINDFURY": "Mega-Windfury", "CLEAVE": "Cleave", "STEALTH": "Stealth",
    "MAGNETIC": "Magnetic",
}


def _kw(k: str) -> str:
    return KW.get(k, k.replace("_", " ").title())


def minion_line(m: dict, i: int) -> str:
    if m.get("type") == "spell":
        return f"  {i}. {m['name']} (Tavern spell)"
    races = "/".join(r.title() for r in m.get("races", [])) or "No tribe"
    kws = ", ".join(_kw(k) for k in m.get("keywords", []))
    gold = "Golden " if m.get("golden") else ""
    s = f"  {i}. {gold}{m['name']} ({races}, tier {m['tier']}) {m['atk']}/{m['hp']}"
    if kws:
        s += f" [{kws}]"
    att = m.get("attached") or []
    if att:
        s += " (attached: " + ", ".join(a.get("name", "?") for a in att) + ")"
    return s


def zone(title: str, minions: list[dict], empty="(empty)") -> str:
    lines = [f"{title}:"]
    if not minions:
        lines.append(f"  {empty}")
    lines += [minion_line(m, i) for i, m in enumerate(minions)]
    return "\n".join(lines)


def card_rules(*minion_lists: list[dict]) -> str:
    seen, lines = set(), ["## Card rules"]
    for ms in minion_lists:
        for m in ms:
            text = (m.get("golden_text") or {}).get("text") if m.get("golden") else None
            text = text or m.get("text") or ""
            key = (m["name"], text)
            if key in seen:
                continue
            seen.add(key)
            lines.append(f"- {'Golden ' if m.get('golden') else ''}{m['name']}: "
                         f"{text if text else '(no text)'}")
    return "\n".join(lines)


def header(me: dict, turn: int) -> str:
    return (f"Turn {turn}. Your health {me['health']}. Tavern tier {me['tier']}. "
            f"Gold {me['gold']}. Upgrade price {me.get('upgrade_price', '?')}.")


def prompt(state: str, rules: str, question: str, answer_format: str, primer=True) -> str:
    parts = [RULES_PRIMER] if primer else []
    parts += ["## State (your view)\n" + state, rules, "## Question\n" + question,
              "## Answer format\n" + answer_format]
    return "\n\n".join(parts)
