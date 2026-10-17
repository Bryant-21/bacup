Function Fragment_Begin(ObjectReference akSpeakerRef)
    Quests:BS01_COMP_Medic:AllyMedicScript medic = GetOwningQuest() as Quests:BS01_COMP_Medic:AllyMedicScript
    If medic == None || medic.MySpells == None || medic.MySpells.Length <= 0 || medic.MySpells[0] == None || medic.Caps001 == None
        Return
    EndIf
    Actor player = Game.GetPlayer()
    If player.GetItemCount(medic.Caps001) < 10
        Return
    EndIf
    player.RemoveItem(medic.Caps001, 10)
    medic.MySpells[0].Cast(player, player)
EndFunction
