Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 6
        Quest targetQuest = masterScript.MoMQuestList[6].MoMQuest
        Int stage = masterScript.CONST_MoM02C_SuccessfullyExtractedData
        If targetQuest != None && !targetQuest.IsStageDone(stage)
            targetQuest.SetStage(stage)
        EndIf
    EndIf
EndFunction
