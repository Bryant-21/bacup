Function Fragment_Begin(ObjectReference akSpeakerRef)
    Expeditions:XPD_Pitt02:QuestScript mission = GetOwningQuest() as Expeditions:XPD_Pitt02:QuestScript
    If mission == None || akSpeakerRef == None
        Return
    EndIf
    Int rescuedStage = 0
    Int diedStage = 0
    If akSpeakerRef == mission.Alias_Actor_HelplessSurvivor_01_Feeble.GetReference()
        rescuedStage = 5010
        diedStage = 5015
    ElseIf akSpeakerRef == mission.Alias_Actor_HelplessSurvivor_02_Middling.GetReference()
        rescuedStage = 5020
        diedStage = 5025
    ElseIf akSpeakerRef == mission.Alias_Actor_HelplessSurvivor_03_Resilient.GetReference()
        rescuedStage = 5030
        diedStage = 5035
    EndIf
    If rescuedStage > 0 && !mission.IsStageDone(rescuedStage) && !mission.IsStageDone(diedStage)
        mission.SetStage(rescuedStage)
    EndIf
EndFunction
