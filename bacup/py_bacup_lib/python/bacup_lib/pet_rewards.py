from __future__ import annotations

import math
import re

from bacup_lib.reputation_catalog import first, reference_id


def node_delivery(node: dict, reader, resolver) -> dict:
    def number(object_id):
        signature, _, fields = reader.fields(object_id)
        value = first(fields, "Value")
        if signature != "GLOB" or not isinstance(value, (int, float)) or not math.isfinite(value) or value <= 0:
            raise ValueError("Invalid pet reward magnitude")
        return value

    def item(object_id, count):
        signature, eid, _ = reader.fields(object_id)
        if count != int(count) or not 0 < count <= 2147483647:
            raise ValueError("Invalid pet reward count")
        if signature == "CNCY" and eid == "PerkScrapCurrency":
            return {"kind": "perk_coins", "count": int(count)}
        if signature == "PPAK" and eid == "PerkCardPack_Pack001":
            return {"kind": "perk_packs", "count": int(count)}
        if signature not in {"ALCH", "AMMO", "ARMO", "BOOK", "MISC", "WEAP", "CNCY"}:
            raise ValueError(f"Unsupported pet reward item: {signature} {eid}")
        base = {("MISC", "Caps001"): "Fallout4.esm:00000F", ("CNCY", "Caps001"): "Fallout4.esm:00000F",
                ("ALCH", "Stimpak"): "Fallout4.esm:023736"}
        form = base.get((signature, eid)) or resolver.resolve("MISC" if signature == "CNCY" else signature, eid)
        if not form:
            raise ValueError(f"Pet reward not converted: {eid}")
        return {"kind": "item", "form": form, "count": int(count)}

    try:
        _, image, _ = reader.fields(node["image_id"])
        match = re.fullmatch(r"WorldPets_Img_All_(PetCarrier|PetProwess)(\d{2})", image)
        if match:
            family, rank = match.groups()
            if node.get("reward_id"):
                _, _, fields = reader.fields(node["reward_id"])
                if first(fields, "RewardsCount") != 0:
                    raise ValueError("Unexpected reward attached to a passive node")
            suffix = "_Damage" if family == "PetProwess" else ""
            magnitude_id, _ = reader.lookup("GLOB", f"WorldPets_Buff_All_{family}{rank}{suffix}")
            return {"kind": "passive", "family": "carry" if family == "PetCarrier" else "prowess",
                    "rank": int(rank), "magnitude": number(magnitude_id)}
        if node.get("reward_id"):
            _, _, fields = reader.fields(node["reward_id"])
            if first(fields, "Entitlement") or first(fields, "UnknownGlobal"):
                raise ValueError("This reward requires an unavailable entitlement or Atoms service")
            rewards = []
            currency = reference_id(first(fields, "QuestRewardCurrencyObject"))
            if currency:
                rewards.append(item(currency, number(reference_id(first(fields, "CurrencyAmountGlobal")))))
            for row in fields.get("RewardedItem", []):
                rewards.append(item(reference_id(row.get("Item")), row.get("Count", 0)))
            if len(rewards) != 1 or first(fields, "XPAmount") not in (None, 0, 0.0):
                raise ValueError("Unsupported compound pet reward")
            return rewards[0]
        match = re.fullmatch(r"WorldPets_Img_(Dog_StimpakFetcher|Cat_BaitsFinder|Radhog_RadMagnet|Deathclaw_MeatLover|Cat_MegaBits|Radhog_NukeHarvest)(\d{2})", image)
        if match:
            family, rank = match.groups()
            species = family.split("_", 1)[0]
            special = family in ("Cat_MegaBits", "Radhog_NukeHarvest")
            quantity = f"WorldPets_Buff_{'Cat_MegaBits' if species == 'Cat' else 'Radhog_RadDration'}{rank}" if special else f"WorldPets_ConsumableBuff_{species}{rank}"
            quantity_id, _ = reader.lookup("GLOB", quantity)
            _, fields = reader.lookup("LVLI", f"WorldPets_LL_{'Buff2' if special else 'ConsumableBuff'}_{species}")
            entries, quantities = fields.get("LVLO", []), fields.get("QuantityGlobal", [])
            if len(entries) != len(quantities):
                raise ValueError("Pet gift list has ambiguous quantities")
            matches = [entry for entry, global_ref in zip(entries, quantities) if reference_id(global_ref) == quantity_id]
            if len(matches) != 1:
                raise ValueError("Pet gift rank is missing or ambiguous")
            reward = item(reference_id(matches[0].get("value")), number(quantity_id))
            if reward["kind"] != "item":
                raise ValueError("Pet gift is not an inventory item")
            event = "fish" if family == "Cat_MegaBits" else "nuke" if special else "fetch"
            result = {"kind": "passive", "family": event, "rank": int(rank), "reward": reward}
            if event != "fish":
                interval_id, _ = reader.lookup("GLOB", "WorldPets_Radhog_Buff2GiftInterval" if event == "nuke" else "WorldPets_ConsumableGiftInterval")
                result["interval"] = number(interval_id)
            return result
        match = re.fullmatch(r"WorldPets_Img_(Dog_BountySniffer|Deathclaw_FunFestation)(0[1-3])", image)
        if match:
            family, rank = match.groups()
            quantity_id, _ = reader.lookup("GLOB", f"WorldPets_Buff_{family}{rank}")
            count = number(quantity_id)
            if count != int(count) or not 0 < count <= 10:
                raise ValueError("Invalid legendary pet reward count")
            return {"kind": "passive", "family": "grunt" if family.startswith("Dog_") else "infestation",
                    "rank": int(rank), "count": int(count)}
        raise ValueError("This passive requires an unavailable gameplay reward route")
    except (KeyError, ValueError, TypeError) as error:
        return {"kind": "unavailable", "reason": str(error)}
