Function GiveDogTreat()
    Actor player = Game.GetPlayer()
    If !Dogfood || player.GetItemCount(Dogfood) < 1
        Return
    EndIf
    player.RemoveItem(Dogfood, 1)
    If ATX_COMP_PetDog_Perk_Spell
        ATX_COMP_PetDog_Perk_Spell.Cast(player, player)
    EndIf
EndFunction
