Function Fragment_Begin(ObjectReference akSpeakerRef)
    Quests:BS01_COMP_Medic:AllyMedicScript medic = GetOwningQuest() as Quests:BS01_COMP_Medic:AllyMedicScript
    If medic == None || medic.MySpells == None || medic.MySpells.Length <= 2 || medic.MySpells[2] == None || medic.Caps001 == None
        Return
    EndIf
    Actor player = Game.GetPlayer()
    If player.GetItemCount(medic.Caps001) < 10
        Return
    EndIf
    player.RemoveItem(medic.Caps001, 10)
    medic.MySpells[2].Cast(player, player)
EndFunction
