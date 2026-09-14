Function SetMoM02CStage(Int aiStage)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 6
        Quest targetQuest = masterScript.MoMQuestList[6].MoMQuest
        If targetQuest != None && !targetQuest.IsStageDone(aiStage)
            targetQuest.SetStage(aiStage)
        EndIf
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        SetMoM02CStage(masterScript.CONST_MoM02C_SuccessfullyExtractedData)
    EndIf
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        SetMoM02CStage(masterScript.CONST_MoM02C_UploadedData)
    EndIf
EndFunction
