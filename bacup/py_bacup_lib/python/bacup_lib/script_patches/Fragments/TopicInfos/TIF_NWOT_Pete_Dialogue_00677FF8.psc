Function Fragment_End(ObjectReference akSpeakerRef)
    NWOT_Pete_DialogueScript dialogueQuest = GetOwningQuest() as NWOT_Pete_DialogueScript
    If dialogueQuest != None && dialogueQuest.AskedAboutBossQuestAV != None
        Game.GetPlayer().SetValue(dialogueQuest.AskedAboutBossQuestAV, 1.0)
    EndIf
EndFunction
