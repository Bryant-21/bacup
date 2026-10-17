; MySpells holds the ally's buff ("Random Buff"); DesiredItems is index-aligned
; with MySpells on the one ally that binds both (Leo: each Nuka flavour -> its
; Nuka-Inspiration). Buffs are cast player-on-player because several bound
; spells use Self targeting.
Function GrantLiteAllyBuff(Int aiSpellIndex)
    If !MySpells || aiSpellIndex < 0 || aiSpellIndex >= MySpells.Length
        Return
    EndIf
    Spell buff = MySpells[aiSpellIndex]
    If buff
        Actor player = Game.GetPlayer()
        buff.Cast(player, player)
    EndIf
EndFunction

Function GiveDesiredItem(Int aiItemIndex)
    If !DesiredItems || aiItemIndex < 0 || aiItemIndex >= DesiredItems.Length
        Return
    EndIf
    Form item = DesiredItems[aiItemIndex]
    Actor player = Game.GetPlayer()
    If !item || player.GetItemCount(item) < 1
        Return
    EndIf
    player.RemoveItem(item, 1)
    If AffinitySystem
        player.ModValue(AffinitySystem, 1.0)
    EndIf
    GrantLiteAllyBuff(aiItemIndex)
EndFunction
