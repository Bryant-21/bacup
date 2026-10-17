Function Fragment_End(ObjectReference akSpeakerRef)
    NWOT_Pete_DialogueScript dialogueQuest = GetOwningQuest() as NWOT_Pete_DialogueScript
    If dialogueQuest != None && dialogueQuest.SeenBossFailedReactionAV != None
        Game.GetPlayer().SetValue(dialogueQuest.SeenBossFailedReactionAV, 1.0)
    EndIf
EndFunction
