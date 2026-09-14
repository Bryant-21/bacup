Function Fragment_End(ObjectReference akSpeakerRef)
    NWOT_Pete_DialogueScript dialogueQuest = GetOwningQuest() as NWOT_Pete_DialogueScript
    If dialogueQuest != None && dialogueQuest.NWOT_Beneath != None && dialogueQuest.BeginQuestStage > 0
        If !dialogueQuest.NWOT_Beneath.IsStageDone(dialogueQuest.BeginQuestStage)
            dialogueQuest.NWOT_Beneath.SetStage(dialogueQuest.BeginQuestStage)
        EndIf
    EndIf
EndFunction
