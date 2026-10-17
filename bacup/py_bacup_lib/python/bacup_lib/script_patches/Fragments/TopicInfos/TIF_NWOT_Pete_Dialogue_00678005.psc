Function Fragment_End(ObjectReference akSpeakerRef)
    NWOT_Pete_DialogueScript dialogueQuest = GetOwningQuest() as NWOT_Pete_DialogueScript
    If dialogueQuest == None || dialogueQuest.Caps001 == None || dialogueQuest.Arcade_Token == None
        Return
    EndIf
    Actor player = Game.GetPlayer()
    If player.GetItemCount(dialogueQuest.Caps001) < 5
        Return
    EndIf
    player.RemoveItem(dialogueQuest.Caps001, 5)
    player.AddItem(dialogueQuest.Arcade_Token, dialogueQuest.TokensToGive)
    If dialogueQuest.BoughtNukacadeTokenAV
        player.SetValue(dialogueQuest.BoughtNukacadeTokenAV, 1.0)
    EndIf
EndFunction
